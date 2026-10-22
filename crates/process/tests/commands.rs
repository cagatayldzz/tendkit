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
