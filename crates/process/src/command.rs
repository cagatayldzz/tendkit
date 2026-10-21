use crate::{
  control::Control,
  environment::{home, search_dirs},
  Sink,
};
use serde_json::json;
use std::{
  env,
  io::Read,
  os::unix::process::CommandExt,
  path::Path,
  process::{Command, Stdio},
  sync::mpsc,
  thread,
  time::{Duration, Instant},
};

/// Each child owns its process group, including any processes it spawns.
pub struct ProbeGroup(pub i32);
impl Drop for ProbeGroup {
  fn drop(&mut self) {
    // SIGKILL only targets this child's group, never the GUI or user's shell.
    unsafe {
      libc::kill(-self.0, libc::SIGKILL);
    }
  }
}

pub fn command(
  program: &Path,
  args: &[String],
  control: &Control,
  sink: &mut Sink,
  stream: bool,
  timeout: Duration,
  extra: &[(&str, &str)],
) -> Result<String, String> {
  control.check()?;
  let mut cmd = Command::new(program);
  cmd.args(args).env_clear().stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).process_group(0).current_dir(home()?);
  for key in ["HOME", "USER", "LOGNAME", "TMPDIR", "HTTPS_PROXY", "HTTP_PROXY", "ALL_PROXY", "NO_PROXY", "SSL_CERT_FILE", "CURL_CA_BUNDLE", "ZDOTDIR"]
  {
    if let Some(value) = env::var_os(key) {
      cmd.env(key, value);
    }
  }
  let mut dirs = vec![program.parent().unwrap_or(Path::new("/usr/bin")).to_owned()];
  dirs.extend(search_dirs());
  cmd
    .env("PATH", env::join_paths(dirs).map_err(|e| e.to_string())?)
    .env("SHELL", "/bin/zsh")
    .env("LANG", "en_US.UTF-8")
    .env("LC_ALL", "en_US.UTF-8")
    .env("NO_COLOR", "1")
    .env("TERM", "dumb")
    .env("CI", "1")
    .env("HOMEBREW_NO_AUTO_UPDATE", "1")
    .env("HOMEBREW_NO_INSTALL_CLEANUP", "1")
    .env("HOMEBREW_NO_SUDO", "1")
    .env("HOMEBREW_NO_ENV_HINTS", "1");
  for (key, value) in extra {
    cmd.env(key, value);
  }
  let mut child = cmd.spawn().map_err(|e| format!("{}: {e}", program.display()))?;
  let group = ProbeGroup(child.id() as i32);
  let (tx, rx) = mpsc::sync_channel(32);
  for mut pipe in [
    Box::new(child.stdout.take().ok_or("Could not open stdout")?) as Box<dyn Read + Send>,
    Box::new(child.stderr.take().ok_or("Could not open stderr")?),
  ] {
    let tx = tx.clone();
    thread::spawn(move || {
      let mut buf = [0; 4096];
      while let Ok(size) = pipe.read(&mut buf) {
        if size == 0 || tx.send(buf[..size].to_vec()).is_err() {
          break;
        }
      }
    });
  }
  drop(tx);
  let start = Instant::now();
  let mut output = Vec::new();
  let result = loop {
    if control.check().is_err() || start.elapsed() > timeout {
      unsafe {
        libc::kill(-(child.id() as i32), libc::SIGTERM);
      }
      thread::sleep(Duration::from_millis(150));
      let _ = child.kill();
      let _ = child.wait();
      break Err(if control.check().is_err() { "Operation cancelled".into() } else { "Command timed out".into() });
    }
    if let Ok(bytes) = rx.recv_timeout(Duration::from_millis(40)) {
      if output.len() < 1_048_576 {
        output.extend_from_slice(&bytes[..bytes.len().min(1_048_576 - output.len())]);
      }
      if stream {
        sink("log", json!({"message": String::from_utf8_lossy(&bytes)}))?;
      }
    }
    if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
      break if status.success() { Ok(()) } else { Err(format!("Command failed ({status})")) };
    }
  };
  drop(group);
  for bytes in rx.iter() {
    if output.len() < 1_048_576 {
      output.extend_from_slice(&bytes[..bytes.len().min(1_048_576 - output.len())]);
    }
    if stream {
      sink("log", json!({"message": String::from_utf8_lossy(&bytes)}))?;
    }
  }
  let output = String::from_utf8_lossy(&output).into_owned();
  result.map_err(|e| if stream { e } else { format!("{e}: {}", output.chars().take(1000).collect::<String>()) })?;
  Ok(output)
}
