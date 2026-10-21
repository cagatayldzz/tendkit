// The single-engine lock uses flock.
#![allow(unsafe_code)]

use serde_json::json;
use std::{
  cell::Cell,
  collections::BTreeSet,
  env,
  fs::{self, OpenOptions},
  os::{
    fd::AsRawFd,
    unix::fs::{DirBuilderExt, OpenOptionsExt},
  },
  path::{Path, PathBuf},
  sync::atomic::Ordering,
  thread,
  time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tendkit_detect::{primary, snapshot, Snapshot, TOOLS};
use tendkit_plan::{build_plan, current_enough, emit_tools, validate_package, PATH_SOURCES};
use tendkit_process::{cancel_on_sigterm, command, curl, home, Control, Sink};
use tendkit_protocol::{InstallPlan, InstallStep, Recipe};
use tendkit_shellrc as shell;

struct WorkDir(PathBuf, Cell<bool>);
impl WorkDir {
  fn new() -> Result<Self, String> {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_nanos();
    let path = env::temp_dir().join(format!("tendkit-{}-{stamp}", std::process::id()));
    fs::DirBuilder::new().mode(0o700).create(&path).map_err(|e| e.to_string())?;
    Ok(Self(path, Cell::new(false)))
  }
}
impl Drop for WorkDir {
  fn drop(&mut self) {
    if !self.1.get() {
      let _ = fs::remove_dir_all(&self.0);
    }
  }
}

fn support_dir() -> Result<PathBuf, String> {
  Ok(home()?.join("Library/Application Support/TendKit"))
}

fn binary(state: &Snapshot, id: &str) -> Result<PathBuf, String> {
  primary(state, id)
    .filter(|i| i.version.is_some())
    .map(|i| PathBuf::from(&i.path))
    .ok_or_else(|| format!("{id} was not found or could not be verified"))
}

fn install_homebrew(plan: &InstallPlan, work: &WorkDir, control: &Control, sink: &mut Sink) -> Result<(), String> {
  if !cfg!(target_arch = "aarch64") {
    return Err("The Homebrew package installer requires Apple Silicon".into());
  }
  let package = plan.package.as_ref().ok_or("Homebrew package information is missing")?;
  validate_package(package)?;
  let path = work.0.join("Homebrew.pkg");
  curl(&package.url, Some(&path), control, sink)?;
  let hash = command(
    Path::new("/usr/bin/shasum"),
    &["-a".into(), "256".into(), path.to_string_lossy().into_owned()],
    control,
    sink,
    false,
    Duration::from_secs(60),
    &[],
  )?;
  if hash.split_whitespace().next() != Some(&package.sha256) {
    return Err("Homebrew package checksum does not match".into());
  }
  let signature = command(
    Path::new("/usr/sbin/pkgutil"),
    &["--check-signature".into(), path.to_string_lossy().into_owned()],
    control,
    sink,
    false,
    Duration::from_secs(60),
    &[],
  )?;
  if !signature.contains("Developer ID Installer:") || !signature.contains("signed by a certificate trusted by") {
    return Err("Could not verify the Homebrew package signature".into());
  }
  sink("log", json!({"message": signature}))?;
  // Installer is an external process: keep its package alive if the GUI cancels
  // or disappears while Installer may still be reading from it.
  work.1.set(true);
  sink(
    "external_action_required",
    json!({"id": "homebrew", "path": path, "message": "Complete the Homebrew installation in macOS Installer, then click Verify and Continue."}),
  )?;
  let start = Instant::now();
  while !control.acknowledged.swap(false, Ordering::SeqCst) {
    control.check()?;
    if start.elapsed() > Duration::from_secs(1800) {
      return Err("Timed out waiting for Installer".into());
    }
    thread::sleep(Duration::from_millis(100));
  }
  binary(&snapshot(), "homebrew")?;
  work.1.set(false);
  Ok(())
}

fn run_step(step: &InstallStep, plan: &InstallPlan, work: &WorkDir, state: &Snapshot, control: &Control, sink: &mut Sink) -> Result<(), String> {
  let timeout = Duration::from_secs(1800);
  match &step.recipe {
    Recipe::Homebrew if step.before.is_none() => install_homebrew(plan, work, control, sink),
    Recipe::Homebrew => command(&binary(state, "homebrew")?, &["update".into()], control, sink, true, timeout, &[]).map(|_| ()),
    Recipe::Brew { package, cask } => {
      let brew = binary(state, "homebrew")?;
      let info = command(
        &brew,
        &["info".into(), "--json=v2".into(), if *cask { "--cask" } else { "--formula" }.into(), package.clone()],
        control,
        sink,
        false,
        Duration::from_secs(120),
        &[],
      )?;
      let info: serde_json::Value = serde_json::from_str(&info).map_err(|e| e.to_string())?;
      let available = if *cask { info["casks"][0]["version"].as_str() } else { info["formulae"][0]["versions"]["stable"].as_str() };
      if available != Some(step.target.as_str()) {
        return Err("The package target version changed. Create a new plan.".into());
      }
      let mut args = vec![if step.before.is_some() { "upgrade" } else { "install" }.into()];
      if *cask {
        args.push("--cask".into());
      }
      args.push(package.clone());
      command(&brew, &args, control, sink, true, timeout, &[]).map(|_| ())
    }
    Recipe::Bun | Recipe::Codex | Recipe::Claude { .. } => {
      let (url, shell, argument) = match &step.recipe {
        Recipe::Bun => ("https://bun.com/install", "/bin/bash", format!("bun-v{}", step.target)),
        Recipe::Codex => ("https://chatgpt.com/codex/install.sh", "/bin/sh", step.target.clone()),
        _ => ("https://claude.ai/install.sh", "/bin/bash", step.target.clone()),
      };
      let script = work.0.join(format!("{}-install.sh", step.id));
      curl(url, Some(&script), control, sink)?;
      let mut args = vec![script.to_string_lossy().into_owned()];
      if matches!(step.recipe, Recipe::Codex) {
        args.push("--release".into());
      }
      args.push(argument);
      let bun_dir = home()?.join(".bun").to_string_lossy().into_owned();
      let codex_dir = step
        .before
        .as_ref()
        .and_then(|i| Path::new(&i.path).parent())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or(home()?.join(".local/bin").to_string_lossy().into_owned());
      let extra = match step.recipe {
        Recipe::Bun => vec![("SHELL", "/bin/false"), ("BUN_INSTALL", bun_dir.as_str())],
        Recipe::Codex => vec![("CODEX_NON_INTERACTIVE", "1"), ("CODEX_INSTALL_DIR", codex_dir.as_str())],
        _ => Vec::new(),
      };
      command(Path::new(shell), &args, control, sink, true, timeout, &extra).map(|_| ())
    }
    Recipe::Opencommit { manager } => {
      let args = if manager == "bun" {
        vec!["add".into(), "--global".into(), format!("opencommit@{}", step.target)]
      } else {
        vec!["install".into(), "--global".into(), format!("opencommit@{}", step.target)]
      };
      command(&binary(state, manager)?, &args, control, sink, true, timeout, &[]).map(|_| ())
    }
  }
}

pub fn execute(plan: InstallPlan, request_id: &str, sink: &mut Sink) -> Result<(), String> {
  if unsafe { libc::getuid() } == 0 {
    return Err("Run the application as a regular user.".into());
  }
  cancel_on_sigterm();
  let support = support_dir()?;
  fs::create_dir_all(&support).map_err(|e| e.to_string())?;
  let lock =
    OpenOptions::new().read(true).write(true).create(true).truncate(false).mode(0o600).open(support.join("run.lock")).map_err(|e| e.to_string())?;
  if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
    return Err("Another TendKit installation is already running.".into());
  }
  let control = Control::listen(request_id.to_owned());
  let mut state = snapshot();
  let expected = build_plan(&plan.selected, plan.configure_shell, &state, plan.package.clone(), |id, _| {
    plan.steps.iter().find(|s| s.id == id).map(|s| s.target.clone()).ok_or("A plan step is missing".into())
  })?;
  if expected != plan {
    return Err("The installation environment changed. No operation was started. Create a new plan.".into());
  }
  let work = WorkDir::new()?;
  let mut failed = BTreeSet::new();
  let mut completed = 0;
  for step in &plan.steps {
    if control.check().is_err() {
      break;
    }
    if step.dependencies.iter().any(|d| failed.contains(d)) {
      failed.insert(step.id.clone());
      sink("step_finished", json!({"id": step.id, "status": "blocked", "message": "A required dependency could not be installed"}))?;
      completed += 1;
      continue;
    }
    sink("step_started", json!({"id": step.id, "message": step.title}))?;
    let result = if step.action.is_skip() { Ok(()) } else { run_step(step, &plan, &work, &state, &control, sink) };
    if control.check().is_ok() {
      state = snapshot();
    }
    let after = primary(&state, &step.id);
    let verified = result.and_then(|_| {
      let installed = after.and_then(|i| i.version.as_deref()).ok_or("Could not verify the installed version")?;
      if step.id != "homebrew" && !current_enough(installed, &step.target) {
        return Err(format!("Expected version {}, found {installed}. Check PATH and the package source.", step.target));
      }
      if step.id == "node" {
        binary(&state, "npm")?;
      }
      Ok(())
    });
    let status = if verified.is_ok() {
      if step.action.is_skip() {
        "skipped"
      } else {
        "succeeded"
      }
    } else if control.check().is_err() {
      "cancelled"
    } else {
      "failed"
    };
    if verified.is_err() {
      failed.insert(step.id.clone());
    }
    sink("step_finished", json!({"id": step.id, "status": status, "before": step.before, "after": after, "message": verified.err()}))?;
    completed += 1;
  }
  if plan.configure_shell && control.check().is_ok() {
    sink("step_started", json!({"id": "shell", "message": "Terminal PATH settings"}))?;
    let blocked = !failed.is_empty();
    let result = if blocked {
      Err("Existing PATH settings were preserved because earlier steps did not complete.".into())
    } else {
      configure_shell(&plan, &state, &failed, &support.join("backups"), sink)
    };
    if result.is_err() {
      failed.insert("shell".into());
    }
    sink(
      "step_finished",
      json!({"id": "shell", "status": if blocked { "blocked" } else if result.is_ok() { "succeeded" } else { "failed" }, "message": result.err()}),
    )?;
  }
  let status = if control.check().is_err() {
    "cancelled"
  } else if failed.is_empty() {
    "succeeded"
  } else {
    "failed"
  };
  if status == "cancelled" {
    state = snapshot();
  }
  emit_tools(&state, sink)?;
  sink("run_finished", json!({"status": status, "completed": completed}))
}

fn configure_shell(plan: &InstallPlan, state: &Snapshot, failed: &BTreeSet<String>, backups: &Path, sink: &mut Sink) -> Result<(), String> {
  let mut paths = Vec::new();
  // Preserve previously configured TendKit paths and add only verified tools.
  for tool in TOOLS {
    if failed.contains(tool.id) {
      continue;
    }
    if let Some(installation) = primary(state, tool.id).filter(|i| i.version.is_some() && PATH_SOURCES.contains(&i.source.as_str())) {
      if let Some(parent) = Path::new(&installation.path).parent() {
        let path = parent.to_string_lossy().into_owned();
        if plan.shell_paths.contains(&path) && !paths.contains(&path) {
          paths.push(path);
        }
      }
    }
  }
  if paths.is_empty() {
    return Ok(());
  }
  for file in &plan.shell_files {
    let updated = shell::update(Path::new(file), &paths, backups)?;
    sink(
      "log",
      json!({"message": format!("{file}: {}", if updated { format!("backed up to {} and PATH updated", backups.display()) } else { "PATH is already configured".into() })}),
    )?;
  }
  Ok(())
}
