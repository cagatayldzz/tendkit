use super::*;
use std::collections::BTreeMap;
#[test]
fn latest_rejects_invalid_selections_before_reading_metadata() {
  let mut events = Vec::new();
  let mut sink = |kind: &str, data| {
    events.push((kind.to_string(), data));
    Ok(())
  };
  assert!(latest(&[], &mut sink).is_err());
  assert!(latest(&["bun".into(), "node".into()], &mut sink).is_err());
  assert!(latest(&["unknown".into()], &mut sink).is_err());
  assert!(events.is_empty());
}

fn fixture(version: &str, source: &str) -> Installation {
  Installation { path: "/fixture/bin/tool".into(), version: Some(version.into()), source: source.into(), error: None }
}
#[test]
fn versions_never_downgrade_stable_installs() {
  assert!(current_enough("26.11.1", "26.9.0"));
  assert!(!current_enough("1.9.0", "1.10.0"));
  assert!(!current_enough("1.2.3-beta", "1.2.3"));
  assert!(!valid_version("1.2.3;touch /tmp/oops"));
}
#[test]
fn external_manager_is_preserved() {
  let state = BTreeMap::from([("node".into(), vec![fixture("26.0.0", "nvm")])]);
  assert!(build_plan(&["node".into()], false, &state, None, |_, _| Ok("26.1.0".into())).is_err());
}
#[test]
fn manageability_follows_recipe_rules() {
  let mut unverified = fixture("1.0.0", "Bun");
  unverified.version = None;
  let state = BTreeMap::from([
    ("node".into(), vec![fixture("26.0.0", "nvm")]),
    ("bun".into(), vec![unverified]),
    ("npm".into(), vec![fixture("11.0.0", "Homebrew")]),
    ("opencommit".into(), vec![fixture("3.0.0", "npm")]),
  ]);
  assert!(!manageable("node", &state));
  assert!(!manageable("bun", &state));
  assert!(!manageable("npm", &state));
  assert!(manageable("opencommit", &state));
  assert!(manageable("homebrew", &state));
  assert!(manageable("codex", &state));
}
#[test]
fn node_update_keeps_formula_and_adds_homebrew_refresh() {
  let mut node = fixture("26.0.0", "Homebrew");
  node.path = "/fixture/Cellar/node@26/26.0.0/bin/node".into();
  let state = BTreeMap::from([("node".into(), vec![node]), ("homebrew".into(), vec![fixture("7.0.9", "Homebrew")])]);
  let plan = build_plan(&["npm".into()], false, &state, None, |_, _| Ok("26.1.0".into())).unwrap();
  assert_eq!(plan.selected, ["node"]);
  assert_eq!(plan.steps[0].action, StepAction::Refresh);
  assert_eq!(plan.steps[1].recipe, Recipe::Brew { package: "node@26".into(), cask: false });
  assert_eq!(plan.steps[1].title, "Node.js + npm");
  assert_eq!(plan.steps[1].dependencies, ["homebrew"]);
  let current = build_plan(&["node".into()], false, &state, None, |_, _| Ok("26.0.0".into())).unwrap();
  assert_eq!(current.steps.len(), 1);
  assert_eq!(current.steps[0].action, StepAction::Skip);
}
#[test]
fn missing_opencommit_adds_bun_dependency() {
  let plan = build_plan(&["opencommit".into()], false, &BTreeMap::new(), None, |_, _| Ok("1.2.3".into())).unwrap();
  assert_eq!(plan.steps.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["bun", "opencommit"]);
  assert_eq!(plan.steps[1].dependencies, ["bun"]);
}
#[test]
fn python_install_adds_homebrew_and_preserves_existing_formula() {
  let package = InstallPackage { version: "7.0.9".into(), url: "unused".into(), sha256: "unused".into() };
  let plan = build_plan(&["python".into()], false, &Snapshot::new(), Some(package), |_, _| Ok("3.14.0".into())).unwrap();
  assert_eq!(plan.steps.iter().map(|step| step.id.as_str()).collect::<Vec<_>>(), ["homebrew", "python"]);
  assert_eq!(plan.steps[1].recipe, Recipe::Brew { package: "python3".into(), cask: false });
  assert_eq!(plan.steps[1].dependencies, ["homebrew"]);

  let mut python = fixture("3.13.0", "Homebrew");
  python.path = "/fixture/Cellar/python@3.13/3.13.0/bin/python3".into();
  let state = Snapshot::from([("python".into(), vec![python]), ("homebrew".into(), vec![fixture("7.0.9", "Homebrew")])]);
  let plan = build_plan(&["python".into()], false, &state, None, |_, _| Ok("3.13.1".into())).unwrap();
  assert_eq!(plan.steps[1].recipe, Recipe::Brew { package: "python@3.13".into(), cask: false });
  assert_eq!(plan.steps[1].action, StepAction::Update);
  for source in ["System", "pyenv", "Python.org", "Unknown"] {
    let state = Snapshot::from([("python".into(), vec![fixture("3.13.0", source)])]);
    assert!(!manageable("python", &state));
    assert!(build_plan(&["python".into()], false, &state, None, |_, _| Ok("3.14.0".into())).is_err());
  }
}
#[test]
fn rust_uses_homebrew_and_preserves_rustup() {
  let package = InstallPackage { version: "7.0.9".into(), url: "unused".into(), sha256: "unused".into() };
  let plan = build_plan(&["rust".into()], false, &Snapshot::new(), Some(package), |_, _| Ok("1.90.0".into())).unwrap();
  assert_eq!(plan.steps.iter().map(|step| step.id.as_str()).collect::<Vec<_>>(), ["homebrew", "rust"]);
  assert_eq!(plan.steps[1].recipe, Recipe::Brew { package: "rust".into(), cask: false });
  assert_eq!(plan.steps[1].dependencies, ["homebrew"]);

  let mut rust = fixture("1.89.0", "Homebrew");
  rust.path = "/fixture/Cellar/rust/1.89.0/bin/rustc".into();
  let state = Snapshot::from([("rust".into(), vec![rust]), ("homebrew".into(), vec![fixture("7.0.9", "Homebrew")])]);
  let plan = build_plan(&["rust".into()], false, &state, None, |_, _| Ok("1.90.0".into())).unwrap();
  assert_eq!(plan.steps[1].recipe, Recipe::Brew { package: "rust".into(), cask: false });
  assert_eq!(plan.steps[1].action, StepAction::Update);
  let state = Snapshot::from([("rust".into(), vec![fixture("1.89.0", "rustup")])]);
  assert!(!manageable("rust", &state));
  assert!(build_plan(&["rust".into()], false, &state, None, |_, _| Ok("1.90.0".into())).is_err());
}
#[test]
fn current_bun_is_skipped_and_package_urls_are_restricted() {
  let state = BTreeMap::from([("bun".into(), vec![fixture("1.4.3", "Bun")])]);
  let plan = build_plan(&["bun".into()], false, &state, None, |_, _| Ok("1.4.3".into())).unwrap();
  assert_eq!(plan.steps[0].action, StepAction::Skip);
  assert!(
    validate_package(&InstallPackage { version: "7.0.9".into(), url: "https://evil.invalid/Homebrew.pkg".into(), sha256: "a".repeat(64) }).is_err()
  );
}
