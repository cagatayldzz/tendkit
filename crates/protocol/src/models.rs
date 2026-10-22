use serde::{Deserialize, Serialize};
use std::fmt;

/// The protocol version both sides must speak.
pub const PROTOCOL_VERSION: u32 = 1;

/// Commands the engine accepts as the first request line.
pub const COMMANDS: &[&str] = &["hello", "scan", "latest", "plan", "execute"];

/// Tool IDs reported by the engine, in display and installation order.
pub const TOOL_IDS: &[&str] = &["homebrew", "node", "npm", "bun", "python", "rust", "codex", "claude", "opencommit"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
  Hello,
  Log,
  Error,
  ToolDetected,
  LatestVersion,
  PlanReady,
  StepStarted,
  StepFinished,
  ExternalActionRequired,
  RunFinished,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
  Succeeded,
  Failed,
  Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
  Succeeded,
  Skipped,
  Failed,
  Blocked,
  Cancelled,
}

impl StepStatus {
  pub fn is_success(self) -> bool {
    matches!(self, Self::Succeeded | Self::Skipped)
  }

  pub fn label(self) -> &'static str {
    match self {
      Self::Succeeded => "Completed",
      Self::Skipped => "Already up to date",
      Self::Blocked => "Dependency blocked",
      Self::Cancelled => "Cancelled",
      Self::Failed => "Failed",
    }
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepAction {
  Install,
  Update,
  Refresh,
  Skip,
}

impl StepAction {
  pub fn is_skip(self) -> bool {
    self == Self::Skip
  }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Installation {
  pub path: String,
  pub version: Option<String>,
  pub source: String,
  pub error: Option<String>,
}

/// How the engine installs or updates one tool. The client displays recipes and
/// sends them back unchanged.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Recipe {
  Homebrew,
  Brew { package: String, cask: bool },
  Bun,
  Codex,
  Claude { channel: String },
  Opencommit { manager: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallPackage {
  pub version: String,
  pub url: String,
  pub sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallStep {
  pub id: String,
  pub title: String,
  pub action: StepAction,
  pub target: String,
  pub recipe: Recipe,
  pub before: Option<Installation>,
  pub dependencies: Vec<String>,
}

impl InstallStep {
  pub fn action_label(&self) -> &'static str {
    match self.action {
      StepAction::Skip => "Up to date · Skip",
      StepAction::Refresh => "Refresh package metadata",
      StepAction::Update => "Update",
      StepAction::Install => "Install",
    }
  }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallPlan {
  pub selected: Vec<String>,
  pub configure_shell: bool,
  pub steps: Vec<InstallStep>,
  pub package: Option<InstallPackage>,
  pub shell_files: Vec<String>,
  pub shell_paths: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct EventData {
  pub id: Option<String>,
  pub installations: Option<Vec<Installation>>,
  pub manageable: Option<bool>,
  /// PATH inherited by the engine, before adding discovery directories.
  pub path_entries: Option<Vec<String>>,
  pub engine_version: Option<String>,
  pub latest_version: Option<String>,
  pub status: Option<String>,
  pub message: Option<String>,
  pub plan: Option<InstallPlan>,
  pub path: Option<String>,
  pub before: Option<Installation>,
  pub after: Option<Installation>,
  pub completed: Option<i64>,
}

impl EventData {
  /// `status` is shared by step and run events; these accessors type it per event.
  pub fn step_status(&self) -> Option<StepStatus> {
    self.status.as_deref().and_then(|s| serde_json::from_value(s.into()).ok())
  }

  pub fn run_status(&self) -> Option<RunStatus> {
    self.status.as_deref().and_then(|s| serde_json::from_value(s.into()).ok())
  }
}

#[derive(Clone, Debug, Deserialize)]
pub struct EngineEvent {
  pub protocol_version: i64,
  pub request_id: String,
  pub sequence: i64,
  #[serde(rename = "type")]
  pub kind: EventType,
  pub data: EventData,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineFailure {
  Unavailable,
  InvalidProtocol,
  Incomplete,
  Failed(String),
  OversizedMessage,
}

impl fmt::Display for EngineFailure {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::Unavailable => f.write_str("The engine was not found. Build the application with scripts/build-app.sh."),
      Self::InvalidProtocol => f.write_str("The application and engine messages are incompatible."),
      Self::Incomplete => f.write_str("The engine exited before the operation completed. Try again."),
      Self::Failed(message) => f.write_str(message),
      Self::OversizedMessage => f.write_str("An engine message exceeded the size limit."),
    }
  }
}

impl std::error::Error for EngineFailure {}

const MAX_LINE: usize = 1_048_576;

/// Pipes may split UTF-8 characters and JSON lines anywhere.
#[derive(Default)]
pub struct EventDecoder {
  buffer: Vec<u8>,
}

impl EventDecoder {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn append(&mut self, data: &[u8]) -> Result<Vec<EngineEvent>, EngineFailure> {
    self.buffer.extend_from_slice(data);
    let mut events = Vec::new();
    while let Some(newline) = self.buffer.iter().position(|&b| b == b'\n') {
      let line = &self.buffer[..newline];
      if line.len() > MAX_LINE {
        return Err(EngineFailure::OversizedMessage);
      }
      if !line.is_empty() {
        events.push(serde_json::from_slice(line).map_err(|_| EngineFailure::InvalidProtocol)?);
      }
      self.buffer.drain(..=newline);
    }
    if self.buffer.len() > MAX_LINE {
      return Err(EngineFailure::OversizedMessage);
    }
    Ok(events)
  }

  pub fn finish(&self) -> Result<(), EngineFailure> {
    if self.buffer.is_empty() {
      Ok(())
    } else {
      Err(EngineFailure::Incomplete)
    }
  }
}

/// The first line the client writes to the engine's stdin.
#[derive(Clone, Debug, Deserialize)]
pub struct EngineRequest {
  pub protocol_version: u32,
  pub request_id: String,
  pub command: String,
  #[serde(default)]
  pub selected: Vec<String>,
  #[serde(default)]
  pub configure_shell: bool,
  pub plan: Option<InstallPlan>,
}
