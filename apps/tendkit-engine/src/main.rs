//! The TendKit installation engine: reads one request from stdin and answers
//! with JSON Lines events on stdout.

mod emit;

use emit::emit;
use serde_json::json;
use std::io::{self, BufRead, Read};
use tendkit_execute::execute;
use tendkit_protocol::{EngineRequest, COMMANDS, PROTOCOL_VERSION};

fn run() -> io::Result<()> {
  let mut line = String::new();
  // One request per process; a new scan launches a fresh, isolated engine.
  io::stdin().lock().take(65537).read_line(&mut line)?;
  if line.len() > 65536 {
    return Err(io::Error::new(io::ErrorKind::InvalidInput, "Request too large"));
  }
  let request: EngineRequest = serde_json::from_str(&line)?;
  let mut sequence = 0;
  if request.protocol_version != PROTOCOL_VERSION || !COMMANDS.contains(&request.command.as_str()) {
    return emit(&request.request_id, &mut sequence, "error", json!({"message": "Unsupported protocol version or command"}));
  }
  let mut sink = |kind: &str, data: serde_json::Value| emit(&request.request_id, &mut sequence, kind, data).map_err(|e| e.to_string());
  sink("hello", json!({"engine_version": env!("CARGO_PKG_VERSION")})).map_err(io::Error::other)?;
  let result = match request.command.as_str() {
    "scan" => tendkit_plan::emit_tools(&tendkit_detect::snapshot(), &mut sink).and_then(|_| sink("run_finished", json!({"status": "succeeded"}))),
    "plan" => tendkit_plan::prepare(&request.selected, request.configure_shell, &mut sink).and_then(|plan| {
      sink("plan_ready", json!({"plan": plan}))?;
      sink("run_finished", json!({"status": "succeeded"}))
    }),
    "latest" => tendkit_plan::latest(&request.selected, &mut sink),
    "execute" => match request.plan {
      Some(plan) => execute(plan, &request.request_id, &mut sink),
      None => Err("Installation plan is missing".into()),
    },
    _ => Ok(()),
  };
  if let Err(message) = result {
    sink("error", json!({"message": message})).map_err(io::Error::other)?;
  }
  Ok(())
}

fn main() {
  if let Err(error) = run() {
    eprintln!("tendkit-engine: {error}");
    std::process::exit(1);
  }
}
