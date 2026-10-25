use crate::{home, Control, Sink};
use serde_json::json;
use std::{
  io::Read,
  path::Path,
  process::{Command, Stdio},
  sync::mpsc,
  thread,
  time::{Duration, Instant},
};

/// Terminates only the owned child's process tree when the operation finishes.
pub struct ProbeGroup(pub i32);

impl Drop for ProbeGroup {
  fn drop(&mut self) {
    let _ = Command::new("taskkill.exe").args(["/PID", &self.0.to_string(), "/T", "/F"]).stdout(Stdio::null()).stderr(Stdio::null()).status();
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
  let mut child = Command::new(program)
    .args(args)
    .envs(extra.iter().copied())
    .current_dir(home()?)
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .map_err(|error| error.to_string())?;
  let group = ProbeGroup(child.id() as i32);
  let (sender, receiver) = mpsc::channel();
  for mut pipe in
    [Box::new(child.stdout.take().ok_or("Missing stdout")?) as Box<dyn Read + Send>, Box::new(child.stderr.take().ok_or("Missing stderr")?)]
  {
    let sender = sender.clone();
    thread::spawn(move || {
      let mut bytes = [0; 4096];
      while let Ok(size) = pipe.read(&mut bytes) {
        if size == 0 || sender.send(bytes[..size].to_vec()).is_err() {
          break;
        }
      }
    });
  }
  drop(sender);
  let started = Instant::now();
  let mut output = Vec::new();
  let result = loop {
    if control.check().is_err() || started.elapsed() > timeout {
      let _ = child.kill();
      let _ = child.wait();
      break Err("Command cancelled or timed out".to_string());
    }
    if let Ok(bytes) = receiver.recv_timeout(Duration::from_millis(30)) {
      let remaining = 1_048_576_usize.saturating_sub(output.len());
      output.extend_from_slice(&bytes[..bytes.len().min(remaining)]);
      if stream {
        sink("log", json!({"message": String::from_utf8_lossy(&bytes)}))?;
      }
    }
    if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
      break if status.success() { Ok(()) } else { Err(format!("Command failed ({status})")) };
    }
  };
  drop(group);
  for bytes in receiver.try_iter() {
    let remaining = 1_048_576_usize.saturating_sub(output.len());
    output.extend_from_slice(&bytes[..bytes.len().min(remaining)]);
  }
  result?;
  Ok(String::from_utf8_lossy(&output).into_owned())
}
