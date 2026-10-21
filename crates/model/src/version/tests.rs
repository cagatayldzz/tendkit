use super::*;

#[test]
fn compares_numeric_components() {
  assert!(is_newer("7.0.10", "7.0.9"));
  assert!(is_newer("26.2.0", "26.1.9"));
  assert!(is_newer("1.5", "1.4.9"));
  assert!(!is_newer("7.0.9", "7.0.9"));
  assert!(!is_newer("1.4", "1.4.0"));
  assert!(!is_newer("3.9.6", "3.14.0"));
}

#[test]
fn ignores_unparseable_and_prerelease_suffixes() {
  assert!(!is_newer("latest", "1.0.0"));
  assert!(!is_newer("2.0.0", "weird"));
  assert!(!is_newer("1.0.0", "1.0.0-beta.1"));
  assert!(is_newer("v1.0.1", "1.0.0"));
}
