//! The tool catalog rows and their update status.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolCategory {
  PackageManager,
  JavaScript,
  Python,
  Rust,
  Ai,
}

impl ToolCategory {
  pub const ALL: [Self; 5] = [Self::PackageManager, Self::JavaScript, Self::Python, Self::Rust, Self::Ai];

  fn key(self) -> &'static str {
    match self {
      Self::PackageManager => "category.package_manager",
      Self::JavaScript => "category.javascript",
      Self::Python => "category.python",
      Self::Rust => "category.rust",
      Self::Ai => "category.ai",
    }
  }

  pub fn title(self) -> String {
    tendkit_i18n::t(self.key())
  }

  pub fn description(self) -> String {
    tendkit_i18n::t(&format!("{}.description", self.key()))
  }
}

#[derive(Clone, Debug)]
pub struct ToolRow {
  pub id: &'static str,
  pub name: &'static str,
  pub installations: Vec<Installation>,
  /// Reported by the engine, which owns the installation source rules.
  pub manageable: bool,
  pub scanned: bool,
}

impl ToolRow {
  /// Display metadata only; the tool list itself comes from `TOOL_IDS`.
  pub(crate) fn new(id: &'static str) -> Self {
    let name = match id {
      "homebrew" => "Homebrew",
      "node" => "Node.js",
      "npm" => "npm",
      "bun" => "Bun",
      "python" => "Python",
      "rust" => "Rust",
      "codex" => "Codex",
      "claude" => "Claude Code",
      "opencommit" => "OpenCommit",
      _ => id,
    };
    Self { id, name, installations: Vec::new(), manageable: false, scanned: false }
  }

  /// A short description in the active language.
  pub fn subtitle(&self) -> String {
    tendkit_i18n::t(&format!("tool.{}.subtitle", self.id))
  }

  pub fn category(&self) -> ToolCategory {
    match self.id {
      "homebrew" => ToolCategory::PackageManager,
      "node" | "npm" | "bun" => ToolCategory::JavaScript,
      "python" => ToolCategory::Python,
      "rust" => ToolCategory::Rust,
      _ => ToolCategory::Ai,
    }
  }

  pub fn primary(&self) -> Option<&Installation> {
    self.installations.first()
  }

  pub fn installed_version(&self) -> Option<&str> {
    self.primary().and_then(|i| i.version.as_deref())
  }
}

/// Whether an installed tool has a newer release, from the latest-version lookups.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateStatus {
  /// The latest version has not been looked up yet.
  Unknown,
  Checking,
  UpToDate,
  Available {
    installed: String,
    latest: String,
  },
  /// The lookup failed, for example when offline or rate limited.
  Unavailable,
}

#[derive(Clone, Debug)]
pub struct StepOutcome {
  pub status: StepStatus,
  pub before: Option<String>,
  pub after: Option<String>,
  pub message: Option<String>,
}
