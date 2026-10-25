use std::{
  fs,
  os::unix::fs::PermissionsExt,
  path::{Path, PathBuf},
  sync::{Arc, Mutex},
};
use tendkit_client::{EngineConnection, EventValidator, Request};
use tendkit_protocol::{EngineEvent, EngineFailure, EventDecoder, EventType, InstallPlan, InstallStep, Installation, Recipe, StepAction, TOOL_IDS};

fn temp_path(name: &str) -> PathBuf {
  static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
  let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  std::env::temp_dir().join(format!("tendkit-{name}-{}-{n}", std::process::id()))
}

fn engine_path() -> PathBuf {
  let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/tendkit-engine");
  assert!(path.is_file(), "Build the Rust engine first: cargo build -p tendkit-engine");
  path
}

fn event(request: &str, sequence: i64, kind: &str, data: &str) -> EngineEvent {
  let mut decoder = EventDecoder::new();
  let line = format!("{{\"protocol_version\":1,\"request_id\":\"{request}\",\"sequence\":{sequence},\"type\":\"{kind}\",\"data\":{data}}}\n");
  decoder.append(line.as_bytes()).unwrap().remove(0)
}

#[test]
fn latest_versions_require_the_requested_tool_and_complete_metadata() {
  let latest = |sequence, kind, data| event("latest", sequence, kind, data);
  let validator = || EventValidator::new("latest", "latest", None, vec!["bun".into()]);
  let mut valid = validator();
  valid.accept(&latest(1, "hello", "{}")).unwrap();
  valid.accept(&latest(2, "latest_version", r#"{"id":"bun","latest_version":"1.4.3"}"#)).unwrap();
  valid.accept(&latest(3, "run_finished", r#"{"status":"succeeded"}"#)).unwrap();
  assert!(valid.completed());

  let mut wrong_tool = validator();
  wrong_tool.accept(&latest(1, "hello", "{}")).unwrap();
  assert!(wrong_tool.accept(&latest(2, "latest_version", r#"{"id":"node","latest_version":"26.0.0"}"#)).is_err());

  let mut missing = validator();
  missing.accept(&latest(1, "hello", "{}")).unwrap();
  assert!(missing.accept(&latest(2, "run_finished", r#"{"status":"succeeded"}"#)).is_err());

  let mut duplicate = validator();
  duplicate.accept(&latest(1, "hello", "{}")).unwrap();
  duplicate.accept(&latest(2, "latest_version", r#"{"id":"bun","latest_version":"1.4.3"}"#)).unwrap();
  assert!(duplicate.accept(&latest(3, "latest_version", r#"{"id":"bun","latest_version":"1.4.3"}"#)).is_err());
}

#[test]
fn fragments_including_split_unicode_characters() {
  let message =
    "{\"protocol_version\":1,\"request_id\":\"test\",\"sequence\":1,\"type\":\"error\",\"data\":{\"message\":\"Version unavailable — retry\"}}\n";
  let mut decoder = EventDecoder::new();
  let mut events = Vec::new();
  for byte in message.as_bytes() {
    events.extend(decoder.append(&[*byte]).unwrap());
  }
  assert_eq!(events.len(), 1);
  assert_eq!(events[0].protocol_version, 1);
  assert_eq!(events[0].data.message.as_deref(), Some("Version unavailable — retry"));
  decoder.finish().unwrap();
}

#[test]
fn multiple_lines_and_incomplete_final_message() {
  let message = "{\"protocol_version\":1,\"request_id\":\"test\",\"sequence\":1,\"type\":\"hello\",\"data\":{}}\n";
  let mut decoder = EventDecoder::new();
  assert_eq!(decoder.append(format!("{message}{message}").as_bytes()).unwrap().len(), 2);
  decoder.append(b"{\"partial\":").unwrap();
  assert_eq!(decoder.finish(), Err(EngineFailure::Incomplete));
}

#[test]
fn unknown_event_types_and_statuses_are_rejected() {
  let mut decoder = EventDecoder::new();
  assert!(decoder.append(b"{\"protocol_version\":1,\"request_id\":\"x\",\"sequence\":1,\"type\":\"surprise\",\"data\":{}}\n").is_err());
  let mut validator = EventValidator::new("scan", "x", None, Vec::new());
  validator.accept(&event("x", 1, "hello", "{}")).unwrap();
  for (offset, id) in TOOL_IDS.iter().enumerate() {
    validator.accept(&event("x", offset as i64 + 2, "tool_detected", &format!(r#"{{"id":"{id}","installations":[]}}"#))).unwrap();
  }
  assert!(validator.accept(&event("x", TOOL_IDS.len() as i64 + 2, "run_finished", r#"{"status":"maybe"}"#)).is_err());
}

#[test]
fn malformed_and_oversized_messages_are_rejected() {
  assert!(EventDecoder::new().append(b"not-json\n").is_err());
  assert_eq!(EventDecoder::new().append(&vec![65u8; 1_048_577]).err(), Some(EngineFailure::OversizedMessage));
}

#[test]
fn incomplete_and_mismatched_streams_are_rejected() {
  let mut validator = EventValidator::new("scan", "valid", None, Vec::new());
  assert!(validator.accept(&event("different", 1, "hello", "{}")).is_err());
  let mut incomplete = EventValidator::new("scan", "valid", None, Vec::new());
  incomplete.accept(&event("valid", 1, "hello", "{}")).unwrap();
  assert!(incomplete.accept(&event("valid", 2, "run_finished", r#"{"status":"succeeded"}"#)).is_err());
}

#[test]
fn actual_rust_engine_completes_scan() {
  let mut events = Vec::new();
  EngineConnection::new().run(&Request::new("scan"), Some(&engine_path()), |e| events.push(e)).unwrap();
  assert_eq!(events.len(), TOOL_IDS.len() + 2);
  assert_eq!(events.first().unwrap().kind, EventType::Hello);
  assert_eq!(events.last().unwrap().kind, EventType::RunFinished);
  let mut ids: Vec<_> = events.iter().filter_map(|e| e.data.id.clone()).collect();
  ids.sort();
  let mut expected: Vec<_> = TOOL_IDS.iter().map(|s| s.to_string()).collect();
  expected.sort();
  assert_eq!(ids, expected);
  assert!(events.iter().filter(|e| e.kind == EventType::ToolDetected).all(|e| e.data.manageable.is_some() && e.data.path_entries.is_some()));
}

fn fixture_plan() -> InstallPlan {
  InstallPlan {
    selected: vec!["homebrew".into()],
    configure_shell: false,
    steps: vec![InstallStep {
      id: "homebrew".into(),
      title: "Homebrew".into(),
      action: StepAction::Install,
      target: "7.0.9".into(),
      recipe: Recipe::Homebrew,
      before: None,
      dependencies: Vec::new(),
    }],
    package: None,
    shell_files: Vec::new(),
    shell_paths: Vec::new(),
  }
}

fn make_fixture(control: &str, event: &str, status: &str) -> PathBuf {
  let path = temp_path("protocol").with_extension("sh");
  let script = r#"#!/bin/bash
set -eu
IFS= read -r request
request_id=$(printf '%s' "$request" | /usr/bin/sed -E 's/.*"request_id":"([^"]+)".*/\1/')
send() {
  printf '{"protocol_version":1,"request_id":"%s","sequence":%s,"type":"%s","data":%s}\n' "$request_id" "$1" "$2" "$3"
}
send 1 hello '{}'
send 2 EVENT '{"id":"homebrew","message":"waiting"}'
IFS= read -r -t 3 response
[[ "$response" == *'"command":"CONTROL"'* ]]
sequence=3
for tool in homebrew node npm bun python rust codex claude opencommit; do
  send "$sequence" tool_detected "{\"id\":\"$tool\",\"installations\":[]}"
  sequence=$((sequence + 1))
done
send "$sequence" run_finished '{"status":"STATUS"}'
"#
  .replace("EVENT", event)
  .replace("CONTROL", control)
  .replace("STATUS", status);
  fs::write(&path, script).unwrap();
  fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
  path
}

fn run_fixture(control: &str, trigger: EventType, status: &str) -> Vec<EngineEvent> {
  let event_name = if control == "continue" { "external_action_required" } else { "log" };
  let fixture = make_fixture(control, event_name, status);
  let connection = Arc::new(EngineConnection::new());
  let events = Arc::new(Mutex::new(Vec::new()));
  let request = Request::new("execute").configure_shell(false).plan(fixture_plan());
  let result = connection.run(&request, Some(&fixture), |e| {
    if e.kind == trigger {
      if control == "continue" {
        connection.acknowledge_installer()
      } else {
        connection.cancel()
      }
    }
    events.lock().unwrap().push(e);
  });
  let _ = fs::remove_file(&fixture);
  result.unwrap();
  Arc::try_unwrap(events).unwrap().into_inner().unwrap()
}

#[test]
fn installer_acknowledgement_streams_without_waiting_for_eof() {
  let events = run_fixture("continue", EventType::ExternalActionRequired, "succeeded");
  assert_eq!(events.last().unwrap().data.status.as_deref(), Some("succeeded"));
}

#[test]
fn cancellation_reaches_engine_over_control_pipe() {
  let events = run_fixture("cancel", EventType::Log, "cancelled");
  assert_eq!(events.last().unwrap().data.status.as_deref(), Some("cancelled"));
}

#[cfg(target_os = "macos")]
#[test]
fn actual_rust_executor_accepts_current_plan_and_rejects_stale_plan() {
  let root = temp_path("execution");
  let bin = root.join(".bun/bin");
  fs::create_dir_all(&bin).unwrap();
  let bun = bin.join("bun");
  fs::write(&bun, "#!/bin/bash\n[[ \"$1\" == --version ]] || exit 99\nprintf '1.4.3\\n'\n").unwrap();
  fs::set_permissions(&bun, fs::Permissions::from_mode(0o700)).unwrap();
  let quote = |value: &str| format!("'{}'", value.replace('\'', "'\\''"));
  let wrapper = root.join("engine-wrapper.sh");
  let script = format!(
    "#!/bin/bash\nexport HOME={}\nexport PATH={}\nexec {}\n",
    quote(&root.to_string_lossy()),
    quote(&format!("{}:/usr/bin:/bin", bin.display())),
    quote(&engine_path().to_string_lossy())
  );
  fs::write(&wrapper, script).unwrap();
  fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
  let plan = |version: &str| InstallPlan {
    selected: vec!["bun".into()],
    configure_shell: false,
    steps: vec![InstallStep {
      id: "bun".into(),
      title: "Bun".into(),
      action: StepAction::Skip,
      target: "1.4.3".into(),
      recipe: Recipe::Bun,
      before: Some(Installation { path: bun.to_string_lossy().into(), version: Some(version.into()), source: "Bun".into(), error: None }),
      dependencies: Vec::new(),
    }],
    package: None,
    shell_files: Vec::new(),
    shell_paths: Vec::new(),
  };
  let mut events = Vec::new();
  let request = |version| Request::new("execute").configure_shell(false).plan(plan(version));
  EngineConnection::new().run(&request("1.4.3"), Some(&wrapper), |e| events.push(e)).unwrap();
  let finished = events.iter().find(|e| e.kind == EventType::StepFinished).unwrap();
  assert_eq!(finished.data.status.as_deref(), Some("skipped"));
  assert_eq!(events.last().unwrap().data.status.as_deref(), Some("succeeded"));

  let stale = EngineConnection::new().run(&request("1.4.2"), Some(&wrapper), |_| {});
  let _ = fs::remove_dir_all(&root);
  let message = stale.expect_err("Stale plan was accepted").to_string();
  assert!(message.contains("environment changed"), "{message}");
}

#[test]
fn one_scan_cannot_change_its_inherited_path_context() {
  let mut validator = EventValidator::new("scan", "doctor", None, vec![]);
  validator.accept(&event("doctor", 1, "hello", "{}")).unwrap();
  validator.accept(&event("doctor", 2, "tool_detected", r#"{"id":"node","installations":[],"path_entries":["/a"]}"#)).unwrap();
  assert!(validator.accept(&event("doctor", 3, "tool_detected", r#"{"id":"bun","installations":[],"path_entries":["/b"]}"#)).is_err());
}

#[cfg(not(target_os = "macos"))]
#[test]
fn actual_rust_executor_rejects_installation_on_unsupported_platforms() {
  let request = Request::new("execute").configure_shell(false).plan(fixture_plan());
  let result = EngineConnection::new().run(&request, Some(&engine_path()), |_| {});
  assert!(
    matches!(&result, Err(EngineFailure::Failed(message)) if message == "Tool installation is currently supported on macOS only."),
    "Expected an explicit platform rejection, received: {result:?}"
  );
}

#[test]
fn actual_rust_engine_preserves_early_rejection_messages() {
  let result = EngineConnection::new().run(&Request::new("unsupported-command"), Some(&engine_path()), |_| {});
  assert_eq!(result, Err(EngineFailure::Failed("Unsupported protocol version or command".into())));
}
