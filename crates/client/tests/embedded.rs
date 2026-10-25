use super::EmbeddedEngine;
use std::fs;

#[test]
fn extracted_engines_are_isolated_and_removed_on_drop() {
  let first = EmbeddedEngine::extract(b"first engine").unwrap();
  let second = EmbeddedEngine::extract(b"second engine").unwrap();
  assert_ne!(first.executable, second.executable);
  assert_eq!(fs::read(&first.executable).unwrap(), b"first engine");
  assert_eq!(fs::read(&second.executable).unwrap(), b"second engine");
  let directory = first.directory.clone();
  drop(first);
  assert!(!directory.exists());
  assert!(second.executable.is_file());
  let directory = second.directory.clone();
  drop(second);
  assert!(!directory.exists());
}
