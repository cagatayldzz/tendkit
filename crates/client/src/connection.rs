use crate::validator::EventValidator;
use serde_json::json;
use std::{
  env,
  io::{Read, Write},
  path::{Path, PathBuf},
  process::{Child, ChildStdin, ChildStdout, Command, Stdio},
  sync::{
    atomic::{AtomicU64, Ordering},
    mpsc, Mutex,
  },
  thread,
  time::{Duration, SystemTime, UNIX_EPOCH},
};
use tendkit_protocol::{EngineEvent, EngineFailure, EventDecoder, InstallPlan, PROTOCOL_VERSION};

const ENGINE_NAME: &str = if cfg!(windows) { "tendkit-engine.exe" } else { "tendkit-engine" };

/// The bundled engine (`Contents/Helpers` on macOS) or one next to the app binary.
pub fn engine_executable() -> Result<PathBuf, EngineFailure> {
  let current = env::current_exe().map_err(|_| EngineFailure::Unavailable)?;
  let dir = current.parent().ok_or(EngineFailure::Unavailable)?;
  let candidates = [dir.join("../Helpers").join(ENGINE_NAME), dir.join(ENGINE_NAME)];
  candidates.into_iter().find(|p| is_executable(p)).ok_or(EngineFailure::Unavailable)
}

fn is_executable(path: &Path) -> bool {
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    path.metadata().map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
  }
  #[cfg(not(unix))]
  {
    path.is_file()
  }
}

#[derive(Clone, Debug)]
pub struct Request {
  pub command: String,
  pub selected: Vec<String>,
  pub configure_shell: bool,
  pub plan: Option<InstallPlan>,
}

impl Request {
  pub fn new(command: &str) -> Self {
    Self { command: command.into(), selected: Vec::new(), configure_shell: true, plan: None }
  }
  pub fn selected(mut self, selected: Vec<String>) -> Self {
    self.selected = selected;
    self
  }
  pub fn configure_shell(mut self, value: bool) -> Self {
    self.configure_shell = value;
    self
  }
  pub fn plan(mut self, plan: InstallPlan) -> Self {
    self.plan = Some(plan);
    self
  }

  fn timeout(&self) -> Duration {
    Duration::from_secs(match self.command.as_str() {
      "execute" => 14400,
      "plan" => 420,
      _ => 90,
    })
  }
}

#[derive(Default)]
struct Control {
  input: Option<ChildStdin>,
  cancelled: bool,
}

/// Each instance owns one engine operation. Control writes are serialized.
pub struct EngineConnection {
  request_id: String,
  control: Mutex<Control>,
}

impl Default for EngineConnection {
  fn default() -> Self {
    Self::new()
  }
}

impl EngineConnection {
  pub fn new() -> Self {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let request_id = format!("{}-{nanos}-{}", std::process::id(), COUNTER.fetch_add(1, Ordering::Relaxed));
    Self { request_id, control: Mutex::new(Control::default()) }
  }

  pub fn request_id(&self) -> &str {
    &self.request_id
  }

  pub fn cancel(&self) {
    let mut control = self.control.lock().unwrap();
    control.cancelled = true;
    self.send(&mut control, "cancel");
  }

  pub fn acknowledge_installer(&self) {
    let mut control = self.control.lock().unwrap();
    self.send(&mut control, "continue");
  }

  fn send(&self, control: &mut Control, command: &str) {
    if let Some(input) = control.input.as_mut() {
      let message = json!({"protocol_version": PROTOCOL_VERSION, "request_id": self.request_id, "command": command});
      let _ = writeln!(input, "{message}").and_then(|_| input.flush());
    }
  }

  /// Runs one request to completion on the calling thread. `on_event` receives
  /// each validated event in order.
  pub fn run(&self, request: &Request, executable: Option<&Path>, mut on_event: impl FnMut(EngineEvent)) -> Result<(), EngineFailure> {
    let executable = match executable {
      Some(path) => path.to_path_buf(),
      None => engine_executable()?,
    };
    let mut child = Command::new(&executable)
      .stdin(Stdio::piped())
      .stdout(Stdio::piped())
      .stderr(Stdio::piped())
      .spawn()
      .map_err(|e| EngineFailure::Failed(format!("Could not start the engine: {e}")))?;
    let pid = child.id();
    let stdout = child.stdout.take().expect("piped stdout");
    let mut stderr = child.stderr.take().expect("piped stderr");
    let diagnostics = thread::spawn(move || {
      let mut data = Vec::new();
      let _ = (&mut stderr).take(65536).read_to_end(&mut data);
      let _ = std::io::copy(&mut stderr, &mut std::io::sink());
      String::from_utf8_lossy(&data).into_owned()
    });

    thread::scope(|scope| {
      // The watchdog cancels, then terminates, an operation that runs too long.
      let (done, finished) = mpsc::channel::<()>();
      scope.spawn(move || {
        if finished.recv_timeout(request.timeout()) == Err(mpsc::RecvTimeoutError::Timeout) {
          self.cancel();
          terminate(pid);
        }
      });
      let result = self.exchange(request, &mut child, stdout, &mut on_event);
      self.control.lock().unwrap().input = None;
      if result.is_err() {
        terminate(pid);
      }
      let status = child.wait();
      let _ = done.send(());
      let diagnostics = diagnostics.join().unwrap_or_default();
      let validator = result?;
      if !status.map(|s| s.success()).unwrap_or(false) {
        return Err(EngineFailure::Failed(if diagnostics.is_empty() {
          "The engine exited unexpectedly or the operation timed out.".into()
        } else {
          diagnostics
        }));
      }
      if !validator.completed() {
        return Err(EngineFailure::Incomplete);
      }
      Ok(())
    })
  }

  /// Sends the request and reads events until the engine closes stdout.
  fn exchange(
    &self,
    request: &Request,
    child: &mut Child,
    mut stdout: ChildStdout,
    on_event: &mut impl FnMut(EngineEvent),
  ) -> Result<EventValidator, EngineFailure> {
    let mut message = json!({
      "protocol_version": PROTOCOL_VERSION, "request_id": self.request_id, "command": request.command,
      "selected": request.selected, "configure_shell": request.configure_shell,
    });
    if let Some(plan) = &request.plan {
      message["plan"] = serde_json::to_value(plan).map_err(|_| EngineFailure::InvalidProtocol)?;
    }
    {
      let mut control = self.control.lock().unwrap();
      let mut input = child.stdin.take().expect("piped stdin");
      writeln!(input, "{message}")
        .and_then(|_| input.flush())
        .map_err(|_| EngineFailure::Failed("Could not send the request to the engine.".into()))?;
      // Only execution keeps stdin open for control messages.
      if request.command == "execute" {
        control.input = Some(input);
      }
      if control.cancelled {
        self.send(&mut control, "cancel");
      }
    }

    let mut decoder = EventDecoder::new();
    let mut validator = EventValidator::new(&request.command, &self.request_id, request.plan.clone(), request.selected.clone());
    let mut buffer = [0u8; 8192];
    loop {
      // A pipe read returns available bytes immediately, so small events never
      // wait for a full buffer, especially while Installer needs an answer.
      let size = match stdout.read(&mut buffer) {
        Ok(size) => size,
        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
        Err(_) => return Err(EngineFailure::Failed("Could not read engine output.".into())),
      };
      if size == 0 {
        break;
      }
      for event in decoder.append(&buffer[..size])? {
        validator.accept(&event)?;
        on_event(event);
      }
    }
    decoder.finish()?;
    Ok(validator)
  }
}

/// SIGTERM lets the engine treat termination as cancellation and stop its
/// process groups; the client never kills the engine's children directly.
fn terminate(pid: u32) {
  #[cfg(unix)]
  unsafe {
    libc::kill(pid as libc::pid_t, libc::SIGTERM);
  }
  #[cfg(not(unix))]
  let _ = pid;
}
