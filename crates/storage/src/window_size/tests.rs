use super::*;

#[test]
fn restores_size_and_recovers_from_invalid_files() {
  let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
  let directory = std::env::temp_dir().join(format!("tendkit-window-size-{}-{timestamp}", std::process::id()));
  let path = directory.join("window-size.json");
  assert_eq!(WindowSize::load(&path), WindowSize::default());
  let size = WindowSize { width: 1320., height: 840. };
  size.save(&path).unwrap();
  assert_eq!(WindowSize::load(&path), size);
  fs::write(&path, br#"{"width":400,"height":300}"#).unwrap();
  assert_eq!(WindowSize::load(&path), WindowSize::MIN);
  for data in ["invalid JSON", r#"{"width":-1,"height":760}"#, r#"{"width":1180}"#] {
    fs::write(&path, data).unwrap();
    assert_eq!(WindowSize::load(&path), WindowSize::default());
  }
  assert!(WindowSize { width: f32::NAN, height: 760. }.save(&path).is_err());
  fs::remove_dir_all(directory).unwrap();
}
