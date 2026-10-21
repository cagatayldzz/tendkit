use super::*;

/// A settings path in a directory no other test uses; tests run in parallel, and
/// a clock alone can return the same value twice.
fn temp_file() -> PathBuf {
  static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
  let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  std::env::temp_dir().join(format!("tendkit-settings-{}-{n}", std::process::id())).join("settings.json")
}

#[test]
fn defaults_apply_to_missing_and_invalid_files() {
  let path = temp_file();
  assert_eq!(AppSettings::load(&path), AppSettings::default());
  fs::create_dir_all(path.parent().unwrap()).unwrap();
  fs::write(&path, "not json").unwrap();
  assert_eq!(AppSettings::load(&path), AppSettings::default());
  fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn saved_values_round_trip_and_new_fields_default() {
  let path = temp_file();
  let settings = AppSettings { check_app_updates: false, configure_shell: false, language: Some("tr".into()) };
  settings.save(&path).unwrap();
  assert_eq!(AppSettings::load(&path), settings);
  fs::write(&path, r#"{"check_app_updates":false}"#).unwrap();
  assert_eq!(AppSettings::load(&path), AppSettings { check_app_updates: false, configure_shell: true, language: None });
  fs::remove_dir_all(path.parent().unwrap()).unwrap();
}
