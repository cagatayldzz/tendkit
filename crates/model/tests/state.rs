use super::strip_ansi;

#[test]
fn strips_ansi_color_sequences() {
  assert_eq!(strip_ansi("\u{1b}[1;32m==>\u{1b}[0m Pouring"), "==> Pouring");
  assert_eq!(strip_ansi("plain — text"), "plain — text");
}

#[test]
fn failed_lookup_never_uses_cached_version_for_update_decisions() {
  let mut model = super::AppModel::new();
  let tool = model.tools.iter_mut().find(|tool| tool.id == "bun").unwrap();
  tool.scanned = true;
  tool.manageable = true;
  tool.installations.push(tendkit_protocol::Installation {
    path: "/fake/bin/bun".into(),
    version: Some("1.0.0".into()),
    source: "Bun".into(),
    error: None,
  });
  model.latest_versions.insert("bun".into(), "2.0.0".into());
  model.latest_version_errors.insert("bun".into(), "Network unavailable".into());
  let tool = model.tool("bun").unwrap();
  assert_eq!(model.update_status(tool), Some(super::UpdateStatus::Unavailable));
  assert!(!model.has_update(tool));
  assert!(!model.can_update(tool));
}
