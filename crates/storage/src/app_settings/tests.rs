use super::*;

fn temp_file() -> PathBuf {
  let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
  std::env::temp_dir().join(format!("tendkit-settings-{}-{nanos}", std::process::id())).join("settings.json")
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
  let settings = AppSettings { check_app_updates: false, configure_shell: false };
  settings.save(&path).unwrap();
  assert_eq!(AppSettings::load(&path), settings);
  fs::write(&path, r#"{"check_app_updates":false}"#).unwrap();
  assert_eq!(AppSettings::load(&path), AppSettings { check_app_updates: false, configure_shell: true });
  fs::remove_dir_all(path.parent().unwrap()).unwrap();
}
