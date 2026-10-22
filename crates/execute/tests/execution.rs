use super::*;
use tendkit_protocol::{Installation, StepAction};

struct FakeSteps {
  state: Snapshot,
  calls: Vec<String>,
  fail: bool,
  cancel: bool,
}

impl StepRunner for FakeSteps {
  fn snapshot(&mut self) -> Snapshot {
    self.state.clone()
  }

  fn run(&mut self, step: &InstallStep, _: &Snapshot, control: &Control, _: &mut Sink) -> Result<(), String> {
    self.calls.push(step.id.clone());
    if self.cancel {
      control.cancelled.store(true, Ordering::SeqCst);
      return Err("Operation cancelled".into());
    }
    if self.fail {
      return Err("Fake command failed".into());
    }
    self.state.insert(
      step.id.clone(),
      vec![Installation { path: "/fake/bin/tool".into(), version: Some(step.target.clone()), source: "Native".into(), error: None }],
    );
    Ok(())
  }
}

fn plan() -> InstallPlan {
  InstallPlan {
    selected: vec!["bun".into(), "opencommit".into()],
    configure_shell: false,
    package: None,
    shell_files: vec![],
    shell_paths: vec![],
    steps: [("bun", vec![]), ("opencommit", vec!["bun".into()])]
      .into_iter()
      .map(|(id, dependencies)| InstallStep {
        id: id.into(),
        title: id.into(),
        action: StepAction::Install,
        target: "1.0.0".into(),
        recipe: Recipe::Bun,
        before: None,
        dependencies,
      })
      .collect(),
  }
}

fn run(fail: bool, cancel: bool) -> (FakeSteps, Vec<serde_json::Value>, usize) {
  let mut runner = FakeSteps { state: Snapshot::new(), calls: vec![], fail, cancel };
  let mut events = vec![];
  let (_, _, completed) = execute_steps(
    &plan(),
    Snapshot::new(),
    &Control::default(),
    &mut |kind, data| {
      if kind == "step_finished" {
        events.push(data);
      }
      Ok(())
    },
    &mut runner,
  )
  .unwrap();
  (runner, events, completed)
}

#[test]
fn dependency_failure_blocks_dependents_without_running_them() {
  let (runner, events, completed) = run(true, false);
  assert_eq!(runner.calls, ["bun"]);
  assert_eq!(events[0]["status"], "failed");
  assert_eq!(events[1]["status"], "blocked");
  assert_eq!(completed, 2);
}

#[test]
fn cancellation_does_not_start_another_step() {
  let (runner, events, completed) = run(false, true);
  assert_eq!(runner.calls, ["bun"]);
  assert_eq!(events[0]["status"], "cancelled");
  assert_eq!(completed, 1);
}

#[test]
fn successful_commands_are_verified_before_dependents_run() {
  let (runner, events, completed) = run(false, false);
  assert_eq!(runner.calls, ["bun", "opencommit"]);
  assert!(events.iter().all(|event| event["status"] == "succeeded"));
  assert_eq!(completed, 2);
}

#[test]
fn dependency_refresh_uses_homebrew_freshness_but_explicit_refresh_is_forced() {
  assert_eq!(homebrew_refresh_args(false), ["update", "--auto-update"]);
  assert_eq!(homebrew_refresh_args(true), ["update"]);
}
