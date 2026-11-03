//! Conservative cleanup proposals and live validation. This module never deletes files.

use std::{
  fs,
  path::{Path, PathBuf},
  time::SystemTime,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CleanupKind {
  BunCache,
  NpmCache,
  PipCache,
  UvCache,
  HomebrewCache,
  XcodeDerivedData,
  RustTarget,
  NodeModules,
  PythonEnvironment,
}

impl CleanupKind {
  pub fn recommended(self) -> bool {
    matches!(self, Self::BunCache | Self::NpmCache | Self::PipCache | Self::UvCache | Self::RustTarget)
  }
}

impl CleanupKind {
  fn cache_path(self, home: &Path) -> Option<PathBuf> {
    match self {
      Self::BunCache => Some(home.join(".bun/install/cache")),
      Self::NpmCache => Some(home.join(".npm/_cacache")),
      Self::PipCache if cfg!(target_os = "macos") => Some(home.join("Library/Caches/pip")),
      Self::PipCache if cfg!(target_os = "linux") => Some(home.join(".cache/pip")),
      Self::UvCache if cfg!(target_os = "macos") => Some(home.join("Library/Caches/uv")),
      Self::UvCache if cfg!(target_os = "linux") => Some(home.join(".cache/uv")),
      Self::HomebrewCache if cfg!(target_os = "macos") => Some(home.join("Library/Caches/Homebrew/downloads")),
      Self::XcodeDerivedData if cfg!(target_os = "macos") => Some(home.join("Library/Developer/Xcode/DerivedData")),
      _ => None,
    }
  }
}

/// Discover only explicitly supported default cache paths. Unknown or custom paths
/// are left to their owning package managers. A failed measurement is reported.
pub fn discover_caches(home: &Path) -> (Vec<CleanupCandidate>, usize) {
  let mut candidates = Vec::new();
  let mut skipped = 0;
  for kind in [
    CleanupKind::BunCache,
    CleanupKind::NpmCache,
    CleanupKind::PipCache,
    CleanupKind::UvCache,
    CleanupKind::HomebrewCache,
    CleanupKind::XcodeDerivedData,
  ] {
    let Some(path) = kind.cache_path(home) else { continue };
    let mut candidate = CleanupCandidate::new(path, 0, kind);
    match fs::symlink_metadata(&candidate.path) {
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
      Err(_) => {
        skipped += 1;
        continue;
      }
      Ok(_) => {}
    }
    let Ok(identity) = candidate.inspect(home) else {
      skipped += 1;
      continue;
    };
    match measure_cache(&candidate.path) {
      Ok(bytes) => {
        candidate.bytes = bytes;
        candidate.identity = Some(identity);
        if candidate.validate(home).is_ok() {
          candidates.push(candidate);
        } else {
          skipped += 1;
        }
      }
      Err(()) => skipped += 1,
    }
  }
  (candidates, skipped)
}

/// Inspect only immediate recognized outputs in an explicitly selected project.
pub fn discover_project(project: &Path, home: &Path) -> Result<(Vec<CleanupCandidate>, usize), String> {
  let home = fs::canonicalize(home).map_err(|error| error.to_string())?;
  if project == home || !project.is_absolute() || !project.starts_with(&home) || fs::canonicalize(project).ok().as_deref() != Some(project) {
    return Err("Select a project directory inside your home without symbolic links.".into());
  }
  let python_project = ["pyproject.toml", "requirements.txt", "setup.py", "Pipfile"].iter().any(|name| regular_file(&project.join(name)));
  let mut candidates = Vec::new();
  let mut skipped = 0;
  for (name, kind, allowed) in [
    ("node_modules", CleanupKind::NodeModules, regular_file(&project.join("package.json"))),
    ("target", CleanupKind::RustTarget, regular_file(&project.join("Cargo.toml"))),
    (".venv", CleanupKind::PythonEnvironment, python_project),
  ] {
    if !allowed {
      continue;
    }
    let path = project.join(name);
    match fs::symlink_metadata(&path) {
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
      Err(_) => {
        skipped += 1;
        continue;
      }
      Ok(_) => {}
    }
    let mut candidate = CleanupCandidate::new(path, 0, kind);
    candidate.selected_project = Some(project.to_path_buf());
    let Ok(identity) = candidate.inspect(&home) else {
      skipped += 1;
      continue;
    };
    match measure_cache(&candidate.path) {
      Ok(bytes) => {
        candidate.bytes = bytes;
        candidate.identity = Some(identity);
        if candidate.validate(&home).is_ok() {
          candidates.push(candidate);
        } else {
          skipped += 1;
        }
      }
      Err(()) => skipped += 1,
    }
  }
  Ok((candidates, skipped))
}

fn measure_cache(root: &Path) -> Result<u64, ()> {
  let started = std::time::Instant::now();
  let mut pending = vec![root.to_path_buf()];
  let mut count = 0_u64;
  let mut bytes = 0_u64;
  #[cfg(unix)]
  let mut seen = std::collections::HashSet::new();
  while let Some(path) = pending.pop() {
    count += 1;
    if count > 100_000 || started.elapsed().as_secs() >= 10 {
      return Err(());
    }
    let metadata = fs::symlink_metadata(&path).map_err(|_| ())?;
    if metadata.file_type().is_symlink() {
      continue;
    }

    #[cfg(unix)]
    {
      use std::os::unix::fs::MetadataExt;

      if !seen.insert((metadata.dev(), metadata.ino())) {
        continue;
      }
      bytes = bytes.saturating_add(metadata.blocks().saturating_mul(512));
    }

    #[cfg(not(unix))]
    {
      bytes = bytes.saturating_add(metadata.len());
    }
    if metadata.is_dir() {
      for entry in fs::read_dir(path).map_err(|_| ())? {
        if count.saturating_add(pending.len() as u64) >= 100_000 || started.elapsed().as_secs() >= 10 {
          return Err(());
        }
        pending.push(entry.map_err(|_| ())?.path());
      }
    }
  }
  Ok(bytes)
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
  selected_project: Option<PathBuf>,
}

impl CleanupCandidate {
  pub(crate) fn new(path: PathBuf, bytes: u64, kind: CleanupKind) -> Self {
    Self { path, bytes, kind, identity: None, selected_project: None }
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
    let known_cache = self.kind.cache_path(&home).is_some_and(|path| path == self.path);
    let selected_project = self.selected_project.as_ref().is_some_and(|project| self.path.parent() == Some(project.as_path()))
      && ["Desktop", "Documents", "Downloads"].iter().any(|name| name.eq_ignore_ascii_case(&first));
    if ["Desktop", "Documents", "Downloads", "Pictures", "Movies", "Music", "Library", "Applications", "Public"]
      .iter()
      .any(|name| name.eq_ignore_ascii_case(&first))
      && !known_cache
      && !selected_project
      || relative
        .components()
        .any(|part| [".git", ".hg", ".svn", ".Trash"].iter().any(|name| part.as_os_str().to_string_lossy().eq_ignore_ascii_case(name)))
      || (first.starts_with('.') && !known_cache)
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
    if self.selected_project.is_some()
      && self.kind == CleanupKind::PythonEnvironment
      && !["pyproject.toml", "requirements.txt", "setup.py", "Pipfile"].iter().any(|name| regular_file(&parent.join(name)))
    {
      return Err("The Python project marker is missing.".into());
    }
    let marker = match self.kind {
      CleanupKind::BunCache
      | CleanupKind::NpmCache
      | CleanupKind::PipCache
      | CleanupKind::UvCache
      | CleanupKind::HomebrewCache
      | CleanupKind::XcodeDerivedData
        if known_cache =>
      {
        None
      }
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
