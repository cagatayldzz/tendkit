//! Conservative cleanup proposals and live validation. This module never deletes files.

use std::{
  fs,
  path::{Path, PathBuf},
  time::SystemTime,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CleanupKind {
  BunCache,
  RustTarget,
  NodeModules,
  PythonEnvironment,
}

impl CleanupKind {
  pub fn recommended(self) -> bool {
    matches!(self, Self::BunCache | Self::RustTarget)
  }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Identity {
  created: Option<SystemTime>,
  #[cfg(unix)]
  device: u64,
  #[cfg(unix)]
  inode: u64,
}

#[derive(Clone, Debug)]
pub struct CleanupCandidate {
  pub path: PathBuf,
  pub bytes: u64,
  pub kind: CleanupKind,
  identity: Option<Identity>,
}

impl CleanupCandidate {
  pub(crate) fn new(path: PathBuf, bytes: u64, kind: CleanupKind) -> Self {
    Self { path, bytes, kind, identity: None }
  }

  /// Reject symlinked ancestors, protected folders and unrecognized layouts.
  fn inspect(&self, home: &Path) -> Result<Identity, String> {
    let home = fs::canonicalize(home).map_err(|error| error.to_string())?;
    if !self.path.is_absolute() || self.path == home || !self.path.starts_with(&home) {
      return Err("Cleanup is limited to directories inside the current user's home.".into());
    }
    let relative = self.path.strip_prefix(&home).map_err(|error| error.to_string())?;
    if relative.components().any(|part| !matches!(part, std::path::Component::Normal(_))) {
      return Err("The cleanup path contains non-normal components.".into());
    }
    let first = relative.components().next().ok_or("Empty cleanup path.")?;
    let first = first.as_os_str().to_string_lossy();
    if ["Desktop", "Documents", "Downloads", "Pictures", "Movies", "Music", "Library", "Applications", "Public"]
      .iter()
      .any(|name| name.eq_ignore_ascii_case(&first))
      || relative
        .components()
        .any(|part| [".git", ".hg", ".svn", ".Trash"].iter().any(|name| part.as_os_str().to_string_lossy().eq_ignore_ascii_case(name)))
      || (first.starts_with('.') && !(self.kind == CleanupKind::BunCache && relative == Path::new(".bun/install/cache")))
    {
      return Err("Protected locations and tool installations are excluded from cleanup.".into());
    }
    let mut cursor = home.clone();
    for component in relative.components() {
      cursor.push(component);
      let metadata = fs::symlink_metadata(&cursor).map_err(|error| error.to_string())?;
      if metadata.is_symlink() || !metadata.is_dir() {
        return Err("Cleanup does not follow symbolic links or non-directory ancestors.".into());
      }
    }
    if fs::canonicalize(&self.path).map_err(|error| error.to_string())? != self.path {
      return Err("The cleanup directory resolves to a different path.".into());
    }
    let parent = self.path.parent().ok_or("The directory has no parent.")?;
    let marker = match self.kind {
      CleanupKind::BunCache if self.path == home.join(".bun/install/cache") => None,
      CleanupKind::RustTarget if self.path.file_name().is_some_and(|name| name == "target") && regular_file(&parent.join("Cargo.toml")) => {
        Some(self.path.join("CACHEDIR.TAG"))
      }
      CleanupKind::NodeModules if self.path.file_name().is_some_and(|name| name == "node_modules") && regular_file(&parent.join("package.json")) => {
        None
      }
      CleanupKind::PythonEnvironment if self.path.file_name().is_some_and(|name| name == ".venv") && regular_file(&self.path.join("pyvenv.cfg")) => {
        None
      }
      _ => return Err("The directory no longer matches a supported cleanup category.".into()),
    };
    if let Some(marker) = marker {
      if !regular_file(&marker) {
        return Err("The Rust build directory has no cache marker.".into());
      }
      use std::io::Read;

      let mut signature = [0; 43];
      fs::File::open(marker).and_then(|mut file| file.read_exact(&mut signature)).map_err(|error| error.to_string())?;
      if &signature != b"Signature: 8a477f597d28d172789f06886806bc55" {
        return Err("The Rust cache marker is invalid.".into());
      }
    }
    let metadata = fs::symlink_metadata(&self.path).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    use std::os::unix::fs::MetadataExt;

    Ok(Identity {
      created: metadata.created().ok(),
      #[cfg(unix)]
      device: metadata.dev(),
      #[cfg(unix)]
      inode: metadata.ino(),
    })
  }

  pub fn validate(&self, home: &Path) -> Result<(), String> {
    let current = self.inspect(home)?;
    if self.identity.as_ref() != Some(&current) {
      return Err("The directory changed since the cleanup preview. Open a new preview.".into());
    }
    Ok(())
  }
}

fn regular_file(path: &Path) -> bool {
  fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_file() && !metadata.is_symlink())
}

/// Validate immediately before calling the supplied OS Trash backend.
pub fn move_to_trash(candidate: &CleanupCandidate, home: &Path, trash: impl FnOnce(&Path) -> Result<(), String>) -> Result<(), String> {
  candidate.validate(home)?;
  trash(&candidate.path)
}

/// Validate proposals and keep only outermost matches, avoiding duplicate moves.
pub fn prepare(mut proposals: Vec<CleanupCandidate>, home: &Path) -> Vec<CleanupCandidate> {
  proposals.sort_by_key(|candidate| candidate.path.components().count());
  let mut candidates: Vec<CleanupCandidate> = Vec::new();
  for mut candidate in proposals {
    if candidates.iter().any(|parent| candidate.path.starts_with(&parent.path)) {
      continue;
    }
    if let Ok(identity) = candidate.inspect(home) {
      candidate.identity = Some(identity);
      candidates.push(candidate);
    }
  }
  candidates.sort_by(|left, right| right.bytes.cmp(&left.bytes).then_with(|| left.path.cmp(&right.path)));
  candidates
}

/// Forget cached sizes before a mutation, including when only part of a batch succeeds.
pub fn invalidate_cache() -> Result<(), String> {
  match fs::remove_file(crate::disk::DiskIndex::cache_path()) {
    Ok(()) => Ok(()),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
    Err(error) => Err(format!("Could not invalidate the disk index: {error}")),
  }
}

#[cfg(test)]
#[path = "../tests/cleanup.rs"]
mod tests;
