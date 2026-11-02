//! The PATH of the user's login shell.
//!
//! An app started from Finder or the Dock inherits launchd's minimal PATH
//! (`/usr/bin:/bin:/usr/sbin:/sbin`), not the one a terminal has. Without this,
//! every tool in `/opt/homebrew/bin` or `~/.cargo/bin` would look as if it were
//! missing from PATH. The engine therefore starts with the login shell's PATH
//! when the app itself only has the minimal one.

use std::{
  ffi::{OsStr, OsString},
  io::Read,
  path::{Path, PathBuf},
  process::{Command, Stdio},
  sync::OnceLock,
  thread,
  time::{Duration, Instant},
};

const MARKER: &str = "__TENDKIT_PATH__";
const MINIMAL: [&str; 4] = ["/usr/bin", "/bin", "/usr/sbin", "/sbin"];
const SHELL_TIMEOUT: Duration = Duration::from_secs(5);

/// The PATH the engine should start with, or `None` to keep the inherited one.
pub fn engine_path() -> Option<&'static OsStr> {
  static RESOLVED: OnceLock<Option<OsString>> = OnceLock::new();
  RESOLVED
    .get_or_init(|| {
      if !cfg!(target_os = "macos") || std::env::var_os("TENDKIT_KEEP_PATH").is_some() {
        return None;
      }
      let inherited = std::env::var_os("PATH").unwrap_or_default();
      if !is_minimal(&inherited) {
        return None;
      }
      query_shell(&login_shell(), SHELL_TIMEOUT)
    })
    .as_deref()
}

/// Whether PATH holds only the system directories a launchd-started app gets.
fn is_minimal(path: &OsStr) -> bool {
  std::env::split_paths(path).filter(|entry| !entry.as_os_str().is_empty()).all(|entry| MINIMAL.iter().any(|minimal| Path::new(minimal) == entry))
}

/// `$SHELL` when it is a shell whose PATH can be read this way, else zsh.
fn login_shell() -> PathBuf {
  let shell = std::env::var_os("SHELL").map(PathBuf::from).filter(|shell| shell.is_absolute() && shell.is_file());
  match shell {
    Some(shell) if matches!(shell.file_name().and_then(OsStr::to_str), Some("zsh" | "bash" | "sh")) => shell,
    _ => PathBuf::from("/bin/zsh"),
  }
}

/// Asks the shell for its PATH. The markers separate it from anything the
/// startup files print.
fn query_shell(shell: &Path, timeout: Duration) -> Option<OsString> {
  let mut child = Command::new(shell)
    .args(["-l", "-i", "-c", &format!("printf '{MARKER}%s{MARKER}' \"$PATH\"")])
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::null())
    .spawn()
    .ok()?;
  let mut stdout = child.stdout.take()?;
  let reader = thread::spawn(move || {
    let mut output = String::new();
    let _ = stdout.read_to_string(&mut output);
    output
  });
  let started = Instant::now();
  loop {
    match child.try_wait() {
      Ok(Some(_)) => break,
      Ok(None) if started.elapsed() < timeout => thread::sleep(Duration::from_millis(20)),
      _ => {
        // A slow or interactive startup file must not block the app.
        let _ = child.kill();
        let _ = child.wait();
        return None;
      }
    }
  }
  parse_marked(&reader.join().ok()?)
}

/// The absolute directories between the markers, joined as a PATH.
fn parse_marked(output: &str) -> Option<OsString> {
  let start = output.find(MARKER)? + MARKER.len();
  let end = start + output[start..].find(MARKER)?;
  let entries: Vec<_> = output[start..end].split(':').filter(|entry| Path::new(entry).is_absolute()).collect();
  if entries.is_empty() {
    return None;
  }
  std::env::join_paths(entries).ok()
}

#[cfg(test)]
#[path = "../tests/login_path.rs"]
mod tests;
