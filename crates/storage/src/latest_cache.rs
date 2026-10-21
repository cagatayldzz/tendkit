use crate::paths::support_dir;
use serde::{Deserialize, Serialize};
use std::{
  collections::HashMap,
  fs,
  path::PathBuf,
  time::{Duration, SystemTime},
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
  pub version: String,
  #[serde(rename = "checked_at", with = "unix_seconds")]
  pub checked_at: SystemTime,
}

mod unix_seconds {
  use serde::{Deserialize, Deserializer, Serializer};
  use std::time::{Duration, SystemTime, UNIX_EPOCH};

  pub fn serialize<S: Serializer>(date: &SystemTime, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_f64(date.duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0))
  }

  pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<SystemTime, D::Error> {
    Ok(UNIX_EPOCH + Duration::from_secs_f64(f64::deserialize(deserializer)?.max(0.0)))
  }
}

/// Persists latest-version lookups across launches so opening the app does not
/// query release metadata (and GitHub's unauthenticated rate limit) every time.
pub struct LatestVersionCache {
  path: PathBuf,
  entries: HashMap<String, Entry>,
}

impl LatestVersionCache {
  pub const LIFETIME: Duration = Duration::from_secs(6 * 60 * 60);

  pub fn default_path() -> PathBuf {
    support_dir().join("latest-versions.json")
  }

  pub fn load(path: PathBuf) -> Self {
    let entries = fs::read(&path).ok().and_then(|data| serde_json::from_slice(&data).ok()).unwrap_or_default();
    Self { path, entries }
  }

  /// The latest version depends on the installation source, so it is part of the key.
  pub fn key(tool: &str, source: Option<&str>) -> String {
    format!("{tool}|{}", source.unwrap_or("none"))
  }

  pub fn entry(&self, key: &str) -> Option<&Entry> {
    self.entries.get(key)
  }

  pub fn fresh(&self, key: &str, now: SystemTime) -> Option<&Entry> {
    self.entries.get(key).filter(|entry| now.duration_since(entry.checked_at).map(|age| age < Self::LIFETIME).unwrap_or(true))
  }

  pub fn store(&mut self, version: &str, key: &str, at: SystemTime) {
    self.entries.insert(key.into(), Entry { version: version.into(), checked_at: at });
    self.save();
  }

  pub fn remove_all(&mut self) {
    self.entries.clear();
    let _ = fs::remove_file(&self.path);
  }

  /// Best effort: a cache that cannot be written only costs another lookup.
  fn save(&self) {
    let Ok(data) = serde_json::to_vec(&self.entries) else { return };
    if let Some(parent) = self.path.parent() {
      let _ = fs::create_dir_all(parent);
    }
    let temporary = self.path.with_extension(format!("tmp-{}", std::process::id()));
    if fs::write(&temporary, data).is_ok() && fs::rename(&temporary, &self.path).is_err() {
      let _ = fs::remove_file(&temporary);
    }
  }
}
