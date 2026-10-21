// Process groups and signals require libc calls.
#![allow(unsafe_code)]

//! Child-process supervision for the engine: cancellable commands that run in
//! their own process groups, network downloads through `curl`, and the search
//! directories used to find developer tools.

mod command;
mod control;
mod environment;
mod network;

pub use command::{command, ProbeGroup};
pub use control::{cancel_on_sigterm, Control};
pub use environment::{home, search_dirs};
pub use network::{curl, get_json};

/// Receives engine events as `(type, data)` pairs.
pub type Sink<'a> = dyn FnMut(&str, serde_json::Value) -> Result<(), String> + 'a;

#[cfg(test)]
mod tests {
  use super::*;
  use std::{
    path::Path,
    sync::atomic::Ordering,
    thread,
    time::{Duration, Instant},
  };
  #[test]
  fn executor_streams_and_cancels_owned_processes() {
    let control = Control::default();
    let cancel = control.clone();
    thread::spawn(move || {
      thread::sleep(Duration::from_millis(300));
      cancel.cancelled.store(true, Ordering::SeqCst);
    });
    let mut logs = Vec::new();
    let start = Instant::now();
    let result = command(
      Path::new("/bin/sh"),
      &["-c".into(), "printf 'hello\\n'; /bin/sleep 30".into()],
      &control,
      &mut |kind, data| {
        if kind == "log" {
          logs.push(data);
        }
        Ok(())
      },
      true,
      Duration::from_secs(10),
      &[],
    );
    assert!(result.unwrap_err().contains("cancelled"));
    assert!(start.elapsed() < Duration::from_secs(3));
    assert!(logs.iter().any(|l| l["message"].as_str().unwrap_or("").contains("hello")));
  }
}
