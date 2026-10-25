use serde_json::json;
use std::{
  collections::BTreeSet,
  env, fs,
  path::{Path, PathBuf},
};
use tendkit_detect::{detect, parse_version, primary, snapshot, tool, Snapshot, TOOLS};
use tendkit_process::{curl, get_json, home, search_dirs, Control, Sink};
use tendkit_protocol::{InstallPackage, InstallPlan, InstallStep, Installation, Recipe, StepAction};

/// Sources whose binary directory TendKit may add to PATH.
pub const PATH_SOURCES: &[&str] = &["Homebrew", "Bun", "Native"];

fn shell_files() -> Result<Vec<String>, String> {
  let home = home()?;
  let dir = env::var_os("ZDOTDIR").map(PathBuf::from).map(|p| if p.is_absolute() { p } else { home.join(p) }).unwrap_or(home);
  Ok([".zprofile", ".zshrc"].iter().map(|f| dir.join(f).to_string_lossy().into_owned()).collect())
}

fn brew_recipe(id: &str, installed: &Installation) -> Result<Recipe, String> {
  let path = PathBuf::from(&installed.path);
  let canonical = fs::canonicalize(&path).unwrap_or(path.clone());
  let parent = path.parent().and_then(|p| fs::canonicalize(p).ok()).unwrap_or_default();
  let candidates = [canonical.to_string_lossy().into_owned(), parent.to_string_lossy().into_owned()];
  for candidate in candidates {
    for (marker, cask) in [("/Cellar/", false), ("/Caskroom/", true)] {
      if let Some((_, rest)) = candidate.split_once(marker) {
        let name = rest.split('/').next().unwrap_or("");
        let allowed = match id {
          "node" => name == "node" || name.strip_prefix("node@").map(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())).unwrap_or(false),
          "python" => {
            name == "python"
              || name == "python3"
              || name.strip_prefix("python@3.").map(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())).unwrap_or(false)
          }
          "rust" => name == "rust",
          "bun" => name == "bun",
          "codex" => name == "codex",
          "claude" => ["claude-code", "claude-code@latest"].contains(&name),
          _ => false,
        };
        if allowed {
          return Ok(Recipe::Brew { package: name.into(), cask });
        }
      }
    }
  }
  Err(format!("{id}: The Homebrew package could not be identified reliably."))
}

fn recipe(id: &str, installed: Option<&Installation>, state: &Snapshot) -> Result<Recipe, String> {
  if cfg!(windows) {
    return Err("Tool installation on Windows is not supported by this build. Use the tool's original package manager.".into());
  }
  if id == "homebrew" {
    return Ok(Recipe::Homebrew);
  }
  if let Some(tool) = installed {
    if tool.version.is_none() {
      return Err(format!("{id}: The installed tool could not be verified. Resolve the scan error first."));
    }
    if tool.source == "Homebrew" {
      return brew_recipe(id, tool);
    }
  }
  match (id, installed.map(|x| x.source.as_str())) {
    ("node", None) => Ok(Recipe::Brew { package: "node".into(), cask: false }),
    ("python", None) => Ok(Recipe::Brew { package: "python3".into(), cask: false }),
    ("rust", None) => Ok(Recipe::Brew { package: "rust".into(), cask: false }),
    ("bun", None | Some("Bun")) => Ok(Recipe::Bun),
    ("codex", None | Some("Native")) => Ok(Recipe::Codex),
    ("claude", None | Some("Native")) => {
      let settings = home()?.join(".claude/settings.json");
      let value: serde_json::Value = fs::read(&settings).ok().and_then(|d| serde_json::from_slice(&d).ok()).unwrap_or(json!({}));
      let channel = match value["autoUpdatesChannel"].as_str() {
        Some("stable") => "stable",
        _ => "latest",
      };
      Ok(Recipe::Claude { channel: channel.into() })
    }
    ("opencommit", None | Some("Bun")) => Ok(Recipe::Opencommit { manager: "bun".into() }),
    ("opencommit", Some("npm")) if primary(state, "npm").and_then(|i| i.version.as_ref()).is_some() => {
      Ok(Recipe::Opencommit { manager: "npm".into() })
    }
    _ => Err(format!("{id}: This installation belongs to another manager. Update it through that manager.")),
  }
}

/// Whether TendKit can install or update this tool. The GUI relies on this
/// instead of duplicating the recipe rules.
pub fn manageable(id: &str, state: &Snapshot) -> bool {
  if !cfg!(target_os = "macos") {
    return false;
  }
  if id == "npm" {
    return false;
  }
  let installed = primary(state, id);
  if installed.is_some_and(|i| i.version.is_none()) {
    return false;
  }
  recipe(id, installed, state).is_ok()
}

pub fn emit_tools(state: &Snapshot, sink: &mut Sink) -> Result<(), String> {
  let path_entries: Vec<String> =
    env::var_os("PATH").map(|value| env::split_paths(&value).map(|path| path.to_string_lossy().into_owned()).collect()).unwrap_or_default();
  for tool in TOOLS {
    sink(
      "tool_detected",
      json!({"id": tool.id, "installations": state.get(tool.id).cloned().unwrap_or_default(), "manageable": manageable(tool.id, state), "path_entries": path_entries}),
    )?;
  }
  Ok(())
}

pub fn valid_version(value: &str) -> bool {
  !value.is_empty()
    && value.len() < 100
    && value.chars().all(|c| c.is_ascii_digit() || ['.', '-', '+'].contains(&c))
    && parse_version(value).as_deref() == Some(value)
}

pub fn current_enough(current: &str, target: &str) -> bool {
  if current == target {
    return true;
  }
  let parse = |v: &str| -> Option<Vec<u64>> {
    if v.contains(['-', '+']) {
      return None;
    }
    v.split('.').map(|p| p.parse::<u64>().ok()).collect()
  };
  match (parse(current), parse(target)) {
    (Some(a), Some(b)) => a >= b,
    _ => false,
  }
}

pub fn build_plan<F>(
  selected: &[String],
  configure_shell: bool,
  state: &Snapshot,
  package: Option<InstallPackage>,
  mut resolve: F,
) -> Result<InstallPlan, String>
where
  F: FnMut(&str, &Recipe) -> Result<String, String>,
{
  let mut selection: BTreeSet<String> = selected.iter().map(|id| if id == "npm" { "node".into() } else { id.clone() }).collect();
  if selection.is_empty() {
    return Err("Select at least one tool.".into());
  }
  if selection.iter().any(|id| tool(id).is_none()) {
    return Err("Unknown tool selection".into());
  }
  let normalized: Vec<_> = selection.iter().cloned().collect();
  if selection.contains("opencommit")
    && matches!(recipe("opencommit", primary(state, "opencommit"), state)?, Recipe::Opencommit { ref manager } if manager == "bun")
    && primary(state, "bun").is_none()
  {
    selection.insert("bun".into());
  }
  let mut steps = Vec::new();
  // npm is verified as part of Node and never planned separately.
  for tool in TOOLS.iter().filter(|t| t.id != "npm") {
    let id = tool.id;
    if !selection.contains(id) {
      continue;
    }
    let before = primary(state, id).cloned();
    let recipe = recipe(id, before.as_ref(), state)?;
    let target = resolve(id, &recipe)?;
    if id != "homebrew" && !valid_version(&target) {
      return Err(format!("{id}: Invalid target version"));
    }
    if let Some(version) = before.as_ref().and_then(|i| i.version.as_deref()) {
      if version.contains(['-', '+']) && version != target {
        return Err(format!("{id}: Prerelease installations are not changed automatically."));
      }
    }
    let action = if id == "homebrew" {
      if before.is_some() {
        StepAction::Refresh
      } else {
        StepAction::Install
      }
    } else if before.as_ref().and_then(|i| i.version.as_deref()).map(|v| current_enough(v, &target)).unwrap_or(false) {
      StepAction::Skip
    } else if before.is_some() {
      StepAction::Update
    } else {
      StepAction::Install
    };
    let dependencies = match &recipe {
      Recipe::Brew { .. } if !action.is_skip() => vec!["homebrew".into()],
      Recipe::Opencommit { manager } if manager == "bun" && selection.contains("bun") => vec!["bun".into()],
      _ => Vec::new(),
    };
    steps.push(InstallStep { id: id.into(), title: tool.title.into(), action, target, recipe, before, dependencies });
  }
  if steps.iter().any(|s| s.dependencies.contains(&"homebrew".into())) && !selection.contains("homebrew") {
    let before = primary(state, "homebrew").cloned();
    steps.insert(
      0,
      InstallStep {
        id: "homebrew".into(),
        title: "Homebrew".into(),
        target: if before.is_none() { package.as_ref().ok_or("Homebrew package not found")?.version.clone() } else { "metadata".into() },
        action: if before.is_none() { StepAction::Install } else { StepAction::Refresh },
        recipe: Recipe::Homebrew,
        before,
        dependencies: Vec::new(),
      },
    );
  }
  let mut shell_paths = Vec::new();
  if configure_shell {
    for tool in TOOLS {
      if let Some(installation) = primary(state, tool.id).filter(|i| i.version.is_some() && PATH_SOURCES.contains(&i.source.as_str())) {
        if let Some(parent) = Path::new(&installation.path).parent() {
          shell_paths.push(parent.to_string_lossy().into_owned());
        }
      }
    }
    for step in &steps {
      if step.before.is_some() {
        continue;
      }
      let path = match step.recipe {
        Recipe::Homebrew | Recipe::Brew { .. } => {
          primary(state, "homebrew").and_then(|i| Path::new(&i.path).parent()).map(Path::to_owned).unwrap_or(PathBuf::from("/opt/homebrew/bin"))
        }
        Recipe::Bun | Recipe::Opencommit { .. } => home()?.join(".bun/bin"),
        _ => home()?.join(".local/bin"),
      };
      shell_paths.push(path.to_string_lossy().into_owned());
    }
    shell_paths.sort();
    shell_paths.dedup();
    if shell_paths.iter().any(|p| p.contains([':', '\n', '\r'])) {
      return Err("Unsupported directory name for PATH".into());
    }
  }
  Ok(InstallPlan {
    selected: normalized,
    configure_shell,
    steps,
    package,
    shell_paths,
    shell_files: if configure_shell { shell_files()? } else { Vec::new() },
  })
}

const HOMEBREW_RELEASE: &str = "https://api.github.com/repos/Homebrew/brew/releases/latest";

fn homebrew_package(sink: &mut Sink) -> Result<InstallPackage, String> {
  let data = get_json(HOMEBREW_RELEASE, sink)?;
  let version = data["tag_name"].as_str().ok_or("Homebrew version is missing")?.to_string();
  let asset = data["assets"].as_array().and_then(|items| items.iter().find(|a| a["name"] == "Homebrew.pkg")).ok_or("Homebrew.pkg not found")?;
  let package = InstallPackage {
    version,
    url: asset["browser_download_url"].as_str().ok_or("Package URL is missing")?.into(),
    sha256: asset["digest"].as_str().and_then(|s| s.strip_prefix("sha256:")).ok_or("Homebrew package checksum is missing")?.into(),
  };
  validate_package(&package)?;
  Ok(package)
}

pub fn validate_package(package: &InstallPackage) -> Result<(), String> {
  if !valid_version(&package.version)
    || package.url != format!("https://github.com/Homebrew/brew/releases/download/{}/Homebrew.pkg", package.version)
    || package.sha256.len() != 64
    || !package.sha256.chars().all(|c| c.is_ascii_hexdigit())
  {
    return Err("Could not verify Homebrew package metadata".into());
  }
  Ok(())
}

fn resolve(recipe: &Recipe, package: Option<&InstallPackage>, sink: &mut Sink) -> Result<String, String> {
  let value = match recipe {
    Recipe::Homebrew => return Ok(package.map(|p| p.version.clone()).unwrap_or("metadata".into())),
    Recipe::Brew { package, cask } => {
      let data = get_json(&format!("https://formulae.brew.sh/api/{}/{package}.json", if *cask { "cask" } else { "formula" }), sink)?;
      if *cask {
        data["version"].as_str().map(str::to_string)
      } else {
        data["versions"]["stable"].as_str().map(str::to_string)
      }
    }
    Recipe::Bun => get_json("https://api.github.com/repos/oven-sh/bun/releases/latest", sink)?["tag_name"]
      .as_str()
      .and_then(|s| s.strip_prefix("bun-v"))
      .map(str::to_string),
    Recipe::Codex => get_json("https://api.github.com/repos/openai/codex/releases/latest", sink)?["tag_name"]
      .as_str()
      .and_then(|s| s.strip_prefix("rust-v"))
      .map(str::to_string),
    Recipe::Claude { channel } => {
      Some(curl(&format!("https://downloads.claude.ai/claude-code-releases/{channel}"), None, &Control::default(), sink)?.trim().to_owned())
    }
    Recipe::Opencommit { .. } => get_json("https://registry.npmjs.org/opencommit/latest", sink)?["version"].as_str().map(str::to_string),
  }
  .ok_or("Target version not found")?;
  if !valid_version(&value) {
    return Err("Invalid target version".into());
  }
  Ok(value)
}

pub fn prepare(selected: &[String], configure_shell: bool, sink: &mut Sink) -> Result<InstallPlan, String> {
  if !cfg!(target_os = "macos") {
    return Err("Tool installation is currently supported on macOS only. Use the original package manager on this platform.".into());
  }
  sink("log", json!({"message": "Checking installation sources and target versions…"}))?;
  let state = snapshot();
  let needs_homebrew = selected.iter().any(|s| ["homebrew", "node", "npm", "python", "rust"].contains(&s.as_str()));
  let package = if needs_homebrew && primary(&state, "homebrew").is_none() { Some(homebrew_package(sink)?) } else { None };
  build_plan(selected, configure_shell, &state, package.clone(), |_, recipe| resolve(recipe, package.as_ref(), sink))
}

/// Version discovery is read-only, including installations owned by other managers.
fn latest_recipe(id: &str, installed: Option<&Installation>, state: &Snapshot) -> Result<Recipe, String> {
  if installed.is_some_and(|i| i.source == "Homebrew") {
    return recipe(id, installed, state);
  }
  match id {
    "node" => Ok(Recipe::Brew { package: "node".into(), cask: false }),
    "python" => Ok(Recipe::Brew { package: "python3".into(), cask: false }),
    "rust" => Ok(Recipe::Brew { package: "rust".into(), cask: false }),
    "bun" => Ok(Recipe::Bun),
    "codex" => Ok(Recipe::Codex),
    "claude" => recipe(id, None, state),
    "opencommit" => Ok(Recipe::Opencommit { manager: "bun".into() }),
    _ => Err("Unknown version source".into()),
  }
}

pub fn latest(selected: &[String], sink: &mut Sink) -> Result<(), String> {
  if selected.len() != 1 {
    return Err("Select exactly one tool to check its latest version.".into());
  }
  let id = &selected[0];
  let entry = tool(id).ok_or("Unknown tool")?;
  let version = match id.as_str() {
    "homebrew" => get_json(HOMEBREW_RELEASE, sink)?["tag_name"].as_str().map(str::to_string).ok_or("Homebrew version is missing")?,
    "npm" => get_json("https://registry.npmjs.org/npm/latest", sink)?["version"].as_str().map(str::to_string).ok_or("npm version is missing")?,
    _ => {
      let dirs = search_dirs();
      let mut state = Snapshot::new();
      state.insert(id.clone(), detect(entry, &dirs));
      let recipe = latest_recipe(id, primary(&state, id), &state)?;
      resolve(&recipe, None, sink)?
    }
  };
  if !valid_version(&version) {
    return Err("Invalid latest version metadata".into());
  }
  sink("latest_version", json!({ "id": id, "latest_version": version }))?;
  sink("run_finished", json!({ "status": "succeeded" }))
}

#[cfg(test)]
#[path = "../tests/planning.rs"]
mod tests;
