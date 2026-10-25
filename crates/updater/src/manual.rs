use crate::{Release, UpdateError};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
  Downloading,
  Verifying,
}

#[derive(Clone, Debug)]
pub struct Staged {
  pub app: PathBuf,
  pub directory: PathBuf,
}

pub fn current_app_bundle() -> Result<PathBuf, UpdateError> {
  Err(UpdateError::new("Install updates from the GitHub release page on this platform."))
}

pub fn prepare(_release: &Release, _current: &Path, _progress: impl FnMut(Stage)) -> Result<Staged, UpdateError> {
  Err(UpdateError::new("Automatic application updates are only supported on macOS."))
}

pub fn install(_staged: &Staged, _current: &Path) -> Result<(), UpdateError> {
  Err(UpdateError::new("Install updates from the GitHub release page on this platform."))
}

pub fn discard(_staged: &Staged) {}
