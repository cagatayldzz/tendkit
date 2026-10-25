//! TendKit's self-update: finds the latest GitHub release, downloads and verifies
//! it (SHA-256, code signature, Gatekeeper, matching team), then swaps the
//! running `.app` bundle and relaunches it.

#[cfg(unix)]
mod install;

#[cfg(windows)]
#[path = "manual.rs"]
mod install;
mod process;
mod release;

pub use install::{current_app_bundle, discard, install, prepare, Stage, Staged};
pub use release::{check, parse_release, repository_slug, Release};

use std::fmt;

/// A failed update step with a message that is safe to show to the user.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateError(String);

impl UpdateError {
  pub(crate) fn new(message: impl Into<String>) -> Self {
    Self(message.into())
  }
}

impl fmt::Display for UpdateError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(&self.0)
  }
}

impl std::error::Error for UpdateError {}

#[cfg(all(test, unix))]
#[path = "../tests/updates.rs"]
mod tests;
