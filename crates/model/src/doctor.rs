use crate::{AppModel, ToolRow};
use std::{collections::BTreeSet, path::Path};
use tendkit_protocol::Installation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DoctorSeverity {
  Passed,
  Info,
  Warning,
  Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DoctorFinding {
  PathConflict,
  VersionMismatch,
  MultipleInstallations,
  ProbeFailed,
  NotOnPath,
  NotInstalled,
  NotScanned,
  PathUnknown,
}

impl DoctorFinding {
  pub fn key(self) -> &'static str {
    match self {
      Self::PathConflict => "path_conflict",
      Self::VersionMismatch => "version_mismatch",
      Self::MultipleInstallations => "multiple",
      Self::ProbeFailed => "probe_failed",
      Self::NotOnPath => "not_on_path",
      Self::NotInstalled => "not_installed",
      Self::NotScanned => "not_scanned",
      Self::PathUnknown => "path_unknown",
    }
  }
}

#[derive(Clone, Debug)]
pub struct DoctorInstallation {
  pub installation: Installation,
  /// Zero-based index in the inherited PATH. None means discovery-only or unknown.
  pub path_rank: Option<usize>,
  pub active: bool,
}

#[derive(Clone, Debug)]
pub struct DoctorCheck {
  pub tool_id: &'static str,
  pub name: &'static str,
  pub severity: DoctorSeverity,
  pub findings: Vec<DoctorFinding>,
  pub installations: Vec<DoctorInstallation>,
}

impl DoctorCheck {
  /// Diagnostic commands are static registry names, never interpolated paths.
  pub fn command(&self) -> &'static str {
    match self.tool_id {
      "homebrew" => "type -a brew\nbrew --version",
      "node" => "type -a node\nnode --version",
      "npm" => "type -a npm\nnpm --version",
      "bun" => "type -a bun\nbun --version",
      "python" => "type -a python3\npython3 --version",
      "rust" => "type -a rustc\nrustc --version",
      "codex" => "type -a codex\ncodex --version",
      "claude" => "type -a claude\nclaude --version",
      "opencommit" => "type -a opencommit oco",
      _ => "",
    }
  }
}

#[derive(Clone, Debug)]
pub struct DoctorReport {
  pub checks: Vec<DoctorCheck>,
  pub relative_path_entries: usize,
}

impl DoctorReport {
  pub fn count(&self, severity: DoctorSeverity) -> usize {
    self.checks.iter().filter(|check| check.severity == severity).count()
  }
}

impl AppModel {
  pub fn doctor_report(&self) -> DoctorReport {
    let mut report = analyze(&self.tools, self.scan_path_entries.as_deref());
    if !self.scan_complete {
      for check in &mut report.checks {
        check.severity = DoctorSeverity::Unavailable;
        check.findings = vec![DoctorFinding::NotScanned];
        check.installations.clear();
      }
    }
    report
  }
}

fn analyze(tools: &[ToolRow], path: Option<&[String]>) -> DoctorReport {
  let checks = tools.iter().map(|tool| check(tool, path)).collect();
  let relative_path_entries = path.unwrap_or_default().iter().filter(|entry| !Path::new(entry).is_absolute()).count();
  DoctorReport { checks, relative_path_entries }
}

fn check(tool: &ToolRow, path: Option<&[String]>) -> DoctorCheck {
  let mut installations: Vec<_> = tool
    .installations
    .iter()
    .map(|installation| {
      let parent = Path::new(&installation.path).parent();
      let path_rank = path.and_then(|entries| entries.iter().position(|entry| Path::new(entry).is_absolute() && Some(Path::new(entry)) == parent));
      DoctorInstallation { installation: installation.clone(), path_rank, active: false }
    })
    .collect();
  if let Some(active) = installations.iter().enumerate().filter_map(|(index, i)| i.path_rank.map(|rank| (index, rank))).min_by_key(|(_, rank)| *rank)
  {
    installations[active.0].active = true;
  }
  let mut findings = Vec::new();
  let severity = if !tool.scanned {
    findings.push(DoctorFinding::NotScanned);
    DoctorSeverity::Unavailable
  } else if installations.is_empty() {
    findings.push(DoctorFinding::NotInstalled);
    DoctorSeverity::Unavailable
  } else {
    let broken = installations.iter().any(|i| i.installation.version.is_none() || i.installation.error.is_some());
    let on_path: Vec<_> = installations.iter().filter(|i| i.path_rank.is_some()).collect();
    let conflict = on_path.iter().enumerate().any(|(index, first)| {
      on_path[index + 1..].iter().any(|second| {
        Path::new(&first.installation.path).file_name() == Path::new(&second.installation.path).file_name()
          && (first.installation.source != second.installation.source
            || matches!((&first.installation.version, &second.installation.version), (Some(a), Some(b)) if a != b))
      })
    });
    let missing_path = path.is_some() && on_path.is_empty();
    if broken {
      findings.push(DoctorFinding::ProbeFailed);
    }
    if conflict {
      findings.push(DoctorFinding::PathConflict);
    }
    if missing_path {
      findings.push(DoctorFinding::NotOnPath);
    }
    let versions: BTreeSet<_> = installations.iter().filter_map(|i| i.installation.version.as_deref()).collect();
    if versions.len() > 1 {
      findings.push(DoctorFinding::VersionMismatch);
    }
    if installations.len() > 1 {
      findings.push(DoctorFinding::MultipleInstallations);
    }
    if path.is_none() {
      findings.push(DoctorFinding::PathUnknown);
    }
    if broken || conflict || missing_path {
      DoctorSeverity::Warning
    } else if findings.is_empty() {
      DoctorSeverity::Passed
    } else {
      DoctorSeverity::Info
    }
  };
  DoctorCheck { tool_id: tool.id, name: tool.name, severity, findings, installations }
}

#[cfg(test)]
#[path = "../tests/doctor.rs"]
mod tests;
