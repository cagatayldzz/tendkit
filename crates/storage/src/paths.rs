use std::{env, path::PathBuf};

pub fn home_dir() -> PathBuf {
  let variable = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
  env::var_os(variable).map(PathBuf::from).unwrap_or_default()
}

/// TendKit's per-user data directory.
pub fn support_dir() -> PathBuf {
  if cfg!(target_os = "macos") {
    home_dir().join("Library/Application Support/TendKit")
  } else if cfg!(windows) {
    env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(home_dir).join("TendKit")
  } else {
    env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home_dir().join(".local/share")).join("tendkit")
  }
}
