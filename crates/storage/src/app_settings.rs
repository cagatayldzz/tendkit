use crate::{fs_util::write_atomic, paths::support_dir};
use serde::{Deserialize, Serialize};
use std::{
  fs, io,
  path::{Path, PathBuf},
};

/// Preferences that persist across launches.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
  /// Look for a new TendKit release when the app starts.
  pub check_app_updates: bool,
  /// Add managed tools to the terminal PATH when a plan is approved.
  pub configure_shell: bool,
  /// The interface language code; `None` follows the language of the computer.
  pub language: Option<String>,
}

impl Default for AppSettings {
  fn default() -> Self {
    Self { check_app_updates: true, configure_shell: true, language: None }
  }
}

impl AppSettings {
  pub fn default_path() -> PathBuf {
    support_dir().join("settings.json")
  }

  /// Missing or unreadable files fall back to the defaults.
  pub fn load(path: &Path) -> Self {
    fs::read(path).ok().and_then(|data| serde_json::from_slice(&data).ok()).unwrap_or_default()
  }

  pub fn save(&self, path: &Path) -> io::Result<()> {
    let data = serde_json::to_vec(self)?;
    if let Some(parent) = path.parent() {
      fs::create_dir_all(parent)?;
    }
    write_atomic(path, &data, 0o600)
  }
}

#[cfg(test)]
mod tests;
