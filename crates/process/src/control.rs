use std::{
  io::{self, BufRead},
  sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
  },
  thread,
};
use tendkit_protocol::PROTOCOL_VERSION;

static TERMINATED: AtomicBool = AtomicBool::new(false);
extern "C" fn terminated(_: libc::c_int) {
  TERMINATED.store(true, Ordering::SeqCst);
}

/// Treat SIGTERM (sent by the GUI watchdog) as a cancellation request.
pub fn cancel_on_sigterm() {
  unsafe {
    libc::signal(libc::SIGTERM, terminated as extern "C" fn(libc::c_int) as libc::sighandler_t);
  }
}

#[derive(Clone, Default)]
pub struct Control {
  pub cancelled: Arc<AtomicBool>,
  pub acknowledged: Arc<AtomicBool>,
}
impl Control {
  pub fn check(&self) -> Result<(), String> {
    if self.cancelled.load(Ordering::SeqCst) || TERMINATED.load(Ordering::SeqCst) {
      Err("Operation cancelled".into())
    } else {
      Ok(())
    }
  }
  pub fn listen(request_id: String) -> Self {
    let control = Self::default();
    let copy = control.clone();
    thread::spawn(move || {
      for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        if line.len() > 65536 {
          break;
        }
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) {
          if value["request_id"] != request_id || value["protocol_version"] != PROTOCOL_VERSION {
            continue;
          }
          match value["command"].as_str() {
            Some("cancel") => {
              copy.cancelled.store(true, Ordering::SeqCst);
            }
            Some("continue") => {
              copy.acknowledged.store(true, Ordering::SeqCst);
            }
            _ => (),
          }
        }
      }
      // Closing the GUI's pipe also cancels its outstanding commands.
      copy.cancelled.store(true, Ordering::SeqCst);
    });
    control
  }
}
