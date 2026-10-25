use crate::{command::command, control::Control, Sink};
use std::{path::Path, time::Duration};

pub fn curl(url: &str, destination: Option<&Path>, control: &Control, sink: &mut Sink) -> Result<String, String> {
  let mut args: Vec<String> = [
    "--proto",
    "=https",
    "--proto-redir",
    "=https",
    "--tlsv1.2",
    "-fSL",
    "--connect-timeout",
    "15",
    "--max-time",
    if destination.is_some() { "600" } else { "45" },
    "--user-agent",
    concat!("TendKit/", env!("CARGO_PKG_VERSION")),
  ]
  .iter()
  .map(|s| s.to_string())
  .collect();
  if let Some(path) = destination {
    args.extend(["--output".into(), path.to_string_lossy().into_owned()]);
  } else {
    args.push("--silent".into());
  }
  args.push(url.into());
  command(
    Path::new(if cfg!(windows) { "curl.exe" } else { "/usr/bin/curl" }),
    &args,
    control,
    sink,
    destination.is_some(),
    Duration::from_secs(610),
    &[],
  )
}

pub fn get_json(url: &str, sink: &mut Sink) -> Result<serde_json::Value, String> {
  serde_json::from_str(&curl(url, None, &Control::default(), sink)?).map_err(|e| format!("Could not read release metadata: {e}"))
}
