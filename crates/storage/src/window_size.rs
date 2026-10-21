use crate::{fs_util::write_atomic, paths::support_dir};
use serde::{Deserialize, Serialize};
use std::{
  fs, io,
  path::{Path, PathBuf},
};

/// Window dimensions in logical pixels, independent of the display scale.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowSize {
  pub width: f32,
  pub height: f32,
}

impl Default for WindowSize {
  fn default() -> Self {
    Self { width: 1180., height: 760. }
  }
}

impl WindowSize {
  pub const MIN: Self = Self { width: 980., height: 620. };

  pub fn default_path() -> PathBuf {
    support_dir().join("window-size.json")
  }

  pub fn load(path: &Path) -> Self {
    fs::read(path)
      .ok()
      .and_then(|data| serde_json::from_slice::<Self>(&data).ok())
      .filter(Self::is_valid)
      .map(|size| Self { width: size.width.max(Self::MIN.width), height: size.height.max(Self::MIN.height) })
      .unwrap_or_default()
  }

  fn is_valid(&self) -> bool {
    self.width.is_finite() && self.height.is_finite() && self.width > 0. && self.height > 0.
  }

  pub fn save(&self, path: &Path) -> io::Result<()> {
    if !self.is_valid() {
      return Err(io::Error::new(io::ErrorKind::InvalidInput, "Invalid window dimensions"));
    }
    let data = serde_json::to_vec(self)?;
    if let Some(parent) = path.parent() {
      fs::create_dir_all(parent)?;
    }
    write_atomic(path, &data, 0o600)
  }
}

#[cfg(test)]
mod tests;
