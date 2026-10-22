use std::{
  collections::{BTreeMap, HashSet},
  env, fs,
  io::Read,
  os::unix::{fs::PermissionsExt, process::CommandExt},
  path::{Path, PathBuf},
  process::{Command, Stdio},
  thread,
  time::{Duration, Instant},
};
use tendkit_process::{search_dirs, ProbeGroup};
use tendkit_protocol::Installation;

pub type Snapshot = BTreeMap<String, Vec<Installation>>;

pub struct Tool {
  pub id: &'static str,
  /// Step title used in installation plans.
  pub title: &'static str,
  pub binaries: &'static [&'static str],
}

/// The single tool registry. Order is also the installation order.
pub const TOOLS: &[Tool] = &[
  Tool { id: "homebrew", title: "Homebrew", binaries: &["brew"] },
  Tool { id: "node", title: "Node.js + npm", binaries: &["node"] },
  Tool { id: "npm", title: "npm", binaries: &["npm"] },
  Tool { id: "bun", title: "Bun", binaries: &["bun"] },
  Tool { id: "python", title: "Python", binaries: &["python3"] },
  Tool { id: "rust", title: "Rust", binaries: &["rustc"] },
  Tool { id: "codex", title: "Codex", binaries: &["codex"] },
  Tool { id: "claude", title: "Claude Code", binaries: &["claude"] },
  Tool { id: "opencommit", title: "OpenCommit", binaries: &["opencommit", "oco"] },
];

pub fn tool(id: &str) -> Option<&'static Tool> {
  TOOLS.iter().find(|t| t.id == id)
}

fn executable(path: &Path) -> bool {
  fs::metadata(path).map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
}

fn source(path: &Path) -> String {
  // rustup proxies may be hard links, including under a custom CARGO_HOME.
  if path.file_name().is_some_and(|name| name == "rustc") {
    use std::os::unix::fs::MetadataExt;
    if let Some(parent) = path.parent() {
      if let (Ok(binary), Ok(rustup)) = (fs::metadata(path), fs::metadata(parent.join("rustup"))) {
        if binary.dev() == rustup.dev() && binary.ino() == rustup.ino() {
          return "rustup".into();
        }
      }
    }
  }
  let real = fs::canonicalize(path).unwrap_or_else(|_| path.to_owned());
  let value = real.to_string_lossy();
  let original = path.to_string_lossy();
  let parent = path.parent().and_then(|p| fs::canonicalize(p).ok()).unwrap_or_default();
  if real.file_name().is_some_and(|name| name == "rustup") || value.contains("/.rustup/toolchains/") {
    return "rustup".into();
  }
  if parent.to_string_lossy().contains("/Cellar/") || ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"].contains(&original.as_ref()) {
    return "Homebrew".into();
  }
  if value.contains("/.codex/packages/standalone/") || value.contains("/.local/share/claude/versions/") {
    return "Native".into();
  }
  if value.contains("/.pyenv/") || original.contains("/.pyenv/") {
    return "pyenv".into();
  }
  if value.contains("/Library/Frameworks/Python.framework/") {
    return "Python.org".into();
  }
  if original == "/usr/bin/python3" || value.contains("/CommandLineTools/") || value.contains("/Xcode.app/") {
    return "System".into();
  }
  for (needle, name) in [
    ("/Cellar/", "Homebrew"),
    ("/Caskroom/", "Homebrew"),
    ("/Homebrew/", "Homebrew"),
    ("/.nvm/", "nvm"),
    ("/fnm/", "fnm"),
    ("/.volta/", "Volta"),
    ("/mise/", "mise"),
    ("/.asdf/", "asdf"),
    ("/.bun/", "Bun"),
    ("/node_modules/", "npm"),
  ] {
    if value.contains(needle) || original.contains(needle) {
      return name.into();
    }
  }
  "Unknown".into()
}

pub fn parse_version(output: &str) -> Option<String> {
  output.split_whitespace().find_map(|word| {
    let candidate = word.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '.' && c != '-' && c != '+');
    let candidate = candidate.strip_prefix('v').unwrap_or(candidate);
    let base = candidate.split(['-', '+']).next()?;
    let parts: Vec<_> = base.split('.').collect();
    if parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())) {
      Some(candidate.to_owned())
    } else {
      None
    }
  })
}

/// How long a `--version` probe may run before it counts as hanging.
const PROBE_TIMEOUT: Duration = Duration::from_secs(4);

fn version(binary: &Path, dirs: &[PathBuf]) -> Result<String, String> {
  version_within(binary, dirs, PROBE_TIMEOUT)
}

fn version_within(binary: &Path, dirs: &[PathBuf], timeout: Duration) -> Result<String, String> {
  let mut command = Command::new(binary);
  command.arg("--version").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).process_group(0);
  // Scripts using /usr/bin/env node also work when launched by Finder.
  let mut child_dirs = vec![binary.parent().unwrap_or(Path::new("/usr/bin")).to_owned()];
  child_dirs.extend_from_slice(dirs);
  if let Ok(path) = env::join_paths(child_dirs) {
    command.env("PATH", path);
  }
  command.env("NO_COLOR", "1").env("HOMEBREW_NO_AUTO_UPDATE", "1");
  let mut child = command.spawn().map_err(|e| e.to_string())?;
  let group = ProbeGroup(child.id() as i32);
  let mut stdout = child.stdout.take().ok_or("Could not open the output pipe")?;
  let reader = thread::spawn(move || {
    let mut output = Vec::new();
    // Bound stored output, but keep draining so the child cannot block on a full pipe.
    let mut buf = [0; 4096];
    while let Ok(n) = stdout.read(&mut buf) {
      if n == 0 {
        break;
      }
      let remaining = 65536_usize.saturating_sub(output.len());
      output.extend_from_slice(&buf[..n.min(remaining)]);
    }
    output
  });
  let started = Instant::now();
  let result = loop {
    match child.try_wait() {
      Ok(Some(status)) => break if status.success() { Ok(()) } else { Err(format!("Version command failed: {status}")) },
      Ok(None) if started.elapsed() < timeout => thread::sleep(Duration::from_millis(30)),
      Ok(None) => {
        let _ = child.kill();
        let _ = child.wait();
        break Err("Version check timed out".into());
      }
      Err(e) => {
        let _ = child.kill();
        let _ = child.wait();
        break Err(e.to_string());
      }
    }
  };
  // Close inherited pipes and clean up children even if the main probe exited.
  drop(group);
  result?;
  let read_deadline = Instant::now() + Duration::from_millis(300);
  while !reader.is_finished() && Instant::now() < read_deadline {
    thread::sleep(Duration::from_millis(10));
  }
  if !reader.is_finished() {
    return Err("Version output did not close".into());
  }
  let output = reader.join().map_err(|_| "Could not read version output")?;
  parse_version(&String::from_utf8_lossy(&output)).ok_or_else(|| "Could not read the version number".into())
}

pub fn detect(tool: &Tool, dirs: &[PathBuf]) -> Vec<Installation> {
  let mut candidates = Vec::new();
  let mut seen = HashSet::new();
  for dir in dirs {
    for name in tool.binaries {
      let path = dir.join(name);
      if !executable(&path) {
        continue;
      }
      let real = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
      if seen.insert(real) {
        candidates.push(path);
      }
    }
  }
  // Probes are independent; run them concurrently so slow binaries do not add up.
  thread::scope(|scope| {
    let probes: Vec<_> = candidates.iter().map(|path| scope.spawn(move || version(path, dirs))).collect();
    candidates
      .iter()
      .zip(probes)
      .map(|(path, probe)| {
        let result = probe.join().unwrap_or_else(|_| Err("Version check failed".into()));
        Installation { source: source(path), path: path.to_string_lossy().into_owned(), version: result.as_ref().ok().cloned(), error: result.err() }
      })
      .collect()
  })
}

pub fn snapshot() -> Snapshot {
  let dirs = search_dirs();
  thread::scope(|scope| {
    let scans: Vec<_> = TOOLS.iter().map(|tool| (tool.id, scope.spawn(|| detect(tool, &dirs)))).collect();
    scans.into_iter().map(|(id, scan)| (id.to_string(), scan.join().unwrap_or_default())).collect()
  })
}

pub fn primary<'a>(snapshot: &'a Snapshot, id: &str) -> Option<&'a Installation> {
  snapshot.get(id)?.first()
}

#[cfg(test)]
#[path = "../tests/detection.rs"]
mod tests;
