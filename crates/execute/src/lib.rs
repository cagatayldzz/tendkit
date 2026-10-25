//! Platform-specific installation execution.

#[cfg(unix)]
mod unix;

#[cfg(unix)]
pub use unix::execute;

#[cfg(windows)]
pub fn execute(_plan: tendkit_protocol::InstallPlan, _request_id: &str, _sink: &mut tendkit_process::Sink) -> Result<(), String> {
  Err("Tool installation on Windows is not supported by this build. Use the tool's original package manager.".into())
}
