use crate::UpdateError;
use std::{path::Path, process::Command};

/// Runs a system tool and returns its combined output, or an error with that
/// output when it exits unsuccessfully.
pub fn run(program: &str, args: &[&str]) -> Result<String, UpdateError> {
  let output = Command::new(program).args(args).output().map_err(|e| UpdateError::new(format!("Could not run {program}: {e}")))?;
  let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
  text.push_str(&String::from_utf8_lossy(&output.stderr));
  if output.status.success() {
    Ok(text)
  } else {
    Err(UpdateError::new(format!("{program} failed: {}", text.trim().chars().take(400).collect::<String>())))
  }
}

const USER_AGENT: &str = concat!("TendKit/", env!("CARGO_PKG_VERSION"));

/// HTTPS GET through curl; returns the status code and body. Redirects stay on HTTPS.
pub fn get(url: &str) -> Result<(u16, String), UpdateError> {
  let output = run(
    "/usr/bin/curl",
    &[
      "--proto",
      "=https",
      "--proto-redir",
      "=https",
      "--tlsv1.2",
      "-sSL",
      "--connect-timeout",
      "15",
      "--max-time",
      "45",
      "--user-agent",
      USER_AGENT,
      "-H",
      "Accept: application/vnd.github+json",
      "--write-out",
      "\n%{http_code}",
      url,
    ],
  )?;
  let (body, status) = output.trim_end().rsplit_once('\n').ok_or_else(|| UpdateError::new("Unexpected response from GitHub."))?;
  let status = status.trim().parse().map_err(|_| UpdateError::new("Unexpected response from GitHub."))?;
  Ok((status, body.to_string()))
}

/// Downloads `url` to `destination`, failing on HTTP errors.
pub fn download(url: &str, destination: &Path) -> Result<(), UpdateError> {
  let destination = destination.to_string_lossy();
  run(
    "/usr/bin/curl",
    &[
      "--proto",
      "=https",
      "--proto-redir",
      "=https",
      "--tlsv1.2",
      "-fSL",
      "--connect-timeout",
      "15",
      "--max-time",
      "900",
      "--user-agent",
      USER_AGENT,
      "--output",
      &destination,
      url,
    ],
  )
  .map(|_| ())
}
