use super::*;

fn installation(path: &str, version: Option<&str>, source: &str) -> Installation {
  Installation { path: path.into(), version: version.map(str::to_owned), source: source.into(), error: None }
}

fn node(installations: Vec<Installation>) -> ToolRow {
  let mut tool = ToolRow::new("node");
  tool.scanned = true;
  tool.installations = installations;
  tool
}

fn paths(entries: &[&str]) -> Vec<String> {
  entries.iter().map(|entry| (*entry).to_owned()).collect()
}

#[test]
fn competing_managers_follow_inherited_path_order() {
  let tool = node(vec![installation("/brew/bin/node", Some("24.0.0"), "Homebrew"), installation("/nvm/bin/node", Some("22.0.0"), "nvm")]);
  let check = check(&tool, Some(&paths(&["/nvm/bin", "/brew/bin"])));
  assert_eq!(check.severity, DoctorSeverity::Warning);
  assert!(check.findings.contains(&DoctorFinding::PathConflict));
  assert!(check.findings.contains(&DoctorFinding::VersionMismatch));
  assert!(!check.installations[0].active);
  assert!(check.installations[1].active);
}

#[test]
fn off_path_versions_are_not_reported_as_path_conflicts() {
  let tool = node(vec![installation("/nvm/bin/node", Some("22.0.0"), "nvm"), installation("/brew/bin/node", Some("24.0.0"), "Homebrew")]);
  let check = check(&tool, Some(&paths(&["/nvm/bin"])));
  assert_eq!(check.severity, DoctorSeverity::Info);
  assert!(check.findings.contains(&DoctorFinding::VersionMismatch));
  assert!(!check.findings.contains(&DoctorFinding::PathConflict));
  assert_eq!(check.installations[1].path_rank, None);
}

#[test]
fn same_version_from_different_managers_still_exposes_shadowing() {
  let tool = node(vec![installation("/a/node", Some("22.0.0"), "nvm"), installation("/b/node", Some("22.0.0"), "Homebrew")]);
  let check = check(&tool, Some(&paths(&["/a", "/b"])));
  assert!(check.findings.contains(&DoctorFinding::PathConflict));
  assert!(!check.findings.contains(&DoctorFinding::VersionMismatch));
}

#[test]
fn identical_versions_from_one_manager_are_informational() {
  let tool = node(vec![installation("/a/node", Some("22.0.0"), "nvm"), installation("/b/node", Some("22.0.0"), "nvm")]);
  let check = check(&tool, Some(&paths(&["/a", "/b"])));
  assert_eq!(check.severity, DoctorSeverity::Info);
  assert_eq!(check.findings, [DoctorFinding::MultipleInstallations]);
}

#[test]
fn first_broken_path_binary_is_not_replaced_by_a_working_fallback() {
  let mut broken = installation("/a/node", None, "nvm");
  broken.error = Some("Version check timed out".into());
  let tool = node(vec![broken, installation("/b/node", Some("24.0.0"), "Homebrew")]);
  let check = check(&tool, Some(&paths(&["/a", "/b"])));
  assert_eq!(check.severity, DoctorSeverity::Warning);
  assert!(check.findings.contains(&DoctorFinding::ProbeFailed));
  assert!(check.installations[0].active);
  assert!(!check.installations[1].active);
}

#[test]
fn a_known_installation_outside_path_is_not_called_active() {
  let tool = node(vec![installation("/brew/bin/node", Some("24.0.0"), "Homebrew")]);
  let check = check(&tool, Some(&[]));
  assert_eq!(check.severity, DoctorSeverity::Warning);
  assert_eq!(check.findings, [DoctorFinding::NotOnPath]);
  assert!(!check.installations[0].active);
}

#[test]
fn older_engine_without_path_context_does_not_invent_a_path_failure() {
  let tool = node(vec![installation("/brew/bin/node", Some("24.0.0"), "Homebrew")]);
  let check = check(&tool, None);
  assert_eq!(check.severity, DoctorSeverity::Info);
  assert_eq!(check.findings, [DoctorFinding::PathUnknown]);
  assert!(!check.installations[0].active);
}

#[test]
fn optional_missing_tools_are_not_environment_failures() {
  let report = analyze(&[node(vec![])], Some(&[]));
  assert_eq!(report.count(DoctorSeverity::Warning), 0);
  assert_eq!(report.count(DoctorSeverity::Unavailable), 1);
  assert_eq!(report.checks[0].findings, [DoctorFinding::NotInstalled]);
}

#[test]
fn incomplete_scans_cannot_report_a_pass() {
  let report = analyze(&[ToolRow::new("node")], Some(&paths(&["/bin"])));
  assert_eq!(report.count(DoctorSeverity::Passed), 0);
  assert_eq!(report.checks[0].findings, [DoctorFinding::NotScanned]);
}

#[test]
fn single_verified_path_installation_passes_without_remote_version_data() {
  let tool = node(vec![installation("/a/node", Some("22.0.0"), "nvm")]);
  let check = check(&tool, Some(&paths(&["/a"])));
  assert_eq!(check.severity, DoctorSeverity::Passed);
  assert!(check.findings.is_empty());
  assert_eq!(check.command(), "type -a node\nnode --version");
}

#[test]
fn relative_and_empty_path_entries_are_reported_and_not_assumed_active() {
  let tool = node(vec![installation("/a/node", Some("22.0.0"), "nvm")]);
  let report = analyze(&[tool], Some(&paths(&["", ".", "relative/bin", "/a"])));
  assert_eq!(report.relative_path_entries, 3);
  assert_eq!(report.checks[0].installations[0].path_rank, Some(3));
  assert!(report.checks[0].installations[0].active);
}

#[test]
fn distinct_command_aliases_do_not_imply_path_shadowing() {
  let mut tool = ToolRow::new("opencommit");
  tool.scanned = true;
  tool.installations = vec![installation("/a/oco", Some("1.0.0"), "Bun"), installation("/b/opencommit", Some("2.0.0"), "npm")];
  let check = check(&tool, Some(&paths(&["/a", "/b"])));
  assert_eq!(check.severity, DoctorSeverity::Info);
  assert!(!check.findings.contains(&DoctorFinding::PathConflict));
}

#[test]
fn failed_completion_cannot_leave_a_healthy_doctor_report() {
  let mut model = AppModel::new();
  model.tools = vec![node(vec![installation("/a/node", Some("22.0.0"), "nvm")])];
  model.scan_path_entries = Some(paths(&["/a"]));
  model.scan_complete = false;
  let report = model.doctor_report();
  assert_eq!(report.count(DoctorSeverity::Passed), 0);
  assert_eq!(report.checks[0].findings, [DoctorFinding::NotScanned]);
  assert!(report.checks[0].installations.is_empty());
  model.scan_complete = true;
  assert_eq!(model.doctor_report().count(DoctorSeverity::Passed), 1);
}
