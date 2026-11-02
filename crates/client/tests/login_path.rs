use super::{is_minimal, parse_marked, query_shell};
use std::{ffi::OsStr, path::Path, time::Duration};

#[test]
fn only_system_directories_count_as_the_minimal_launchd_path() {
  assert!(is_minimal(OsStr::new("/usr/bin:/bin:/usr/sbin:/sbin")));
  assert!(is_minimal(OsStr::new("")));
  assert!(!is_minimal(OsStr::new("/opt/homebrew/bin:/usr/bin")));
  assert!(!is_minimal(OsStr::new("/usr/bin:/usr/local/bin")));
}

#[test]
fn the_path_is_read_between_markers_and_other_output_is_ignored() {
  let output = "Last login: today\n__TENDKIT_PATH__/opt/homebrew/bin:/usr/bin:relative:/bin__TENDKIT_PATH__\nbye";
  assert_eq!(parse_marked(output).unwrap(), OsStr::new("/opt/homebrew/bin:/usr/bin:/bin"));
}

#[test]
fn missing_markers_or_no_absolute_directory_give_nothing() {
  assert!(parse_marked("/usr/bin:/bin").is_none());
  assert!(parse_marked("__TENDKIT_PATH__/usr/bin").is_none());
  assert!(parse_marked("__TENDKIT_PATH__relative:other__TENDKIT_PATH__").is_none());
}

#[cfg(unix)]
#[test]
fn a_real_shell_reports_an_absolute_path() {
  let path = query_shell(Path::new("/bin/sh"), Duration::from_secs(10)).expect("sh prints its PATH");
  assert!(std::env::split_paths(&path).all(|entry| entry.is_absolute()));
}

#[cfg(unix)]
#[test]
fn a_missing_shell_gives_nothing() {
  assert!(query_shell(Path::new("/nonexistent/shell"), Duration::from_secs(1)).is_none());
}
