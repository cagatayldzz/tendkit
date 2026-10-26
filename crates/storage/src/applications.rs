//! Read application bundles from standard macOS locations and validate removal.

use std::{
  collections::HashSet,
  fs,
  path::{Path, PathBuf},
  time::{Instant, SystemTime},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApplicationOrigin {
  System,
  Shared,
  User,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemovalBlock {
  System,
  SymbolicLink,
  InvalidBundle,
  CurrentApplication,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Identity {
  created: Option<SystemTime>,
  modified: Option<SystemTime>,
  length: u64,
  #[cfg(unix)]
  device: u64,
  #[cfg(unix)]
  inode: u64,
}

#[derive(Clone, Debug)]
pub struct InstalledApplication {
  pub name: String,
  pub path: PathBuf,
  pub version: Option<String>,
  pub bundle_id: Option<String>,
  pub bytes: Option<u64>,
  pub origin: ApplicationOrigin,
  pub blocked: Option<RemovalBlock>,
  identity: Option<Identity>,
  info_identity: Option<Identity>,
}

#[derive(Default)]
pub struct ApplicationInventory {
  pub applications: Vec<InstalledApplication>,
  pub incomplete: bool,
}

#[derive(Default)]
struct BundleInfo {
  name: Option<String>,
  version: Option<String>,
  bundle_id: Option<String>,
}

fn roots(home: &Path) -> Vec<(PathBuf, ApplicationOrigin)> {
  vec![
    (PathBuf::from("/Applications"), ApplicationOrigin::Shared),
    (home.join("Applications"), ApplicationOrigin::User),
    (PathBuf::from("/System/Applications"), ApplicationOrigin::System),
    (PathBuf::from("/System/Library/CoreServices/Applications"), ApplicationOrigin::System),
  ]
}

pub fn scan_installed() -> ApplicationInventory {
  scan_roots(&roots(&crate::paths::home_dir()), read_bundle_info)
}

fn scan_roots(roots: &[(PathBuf, ApplicationOrigin)], read_info: impl Fn(&Path) -> Option<BundleInfo>) -> ApplicationInventory {
  let started = Instant::now();
  let mut inventory = ApplicationInventory::default();
  let mut pending: Vec<_> = roots.iter().map(|(path, origin)| (path.clone(), *origin, 0)).collect();
  let mut visited = HashSet::new();
  let current = std::env::current_exe().ok();
  let mut size_budget = 200_000;
  'scan: while let Some((directory, origin, depth)) = pending.pop() {
    if started.elapsed().as_secs() >= 30 || visited.len() >= 10_000 {
      inventory.incomplete = true;
      break;
    }
    if !visited.insert(directory.clone()) {
      continue;
    }
    let children = match fs::read_dir(&directory) {
      Ok(children) => children,
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
      Err(_) => {
        inventory.incomplete = true;
        continue;
      }
    };
    for child in children {
      if started.elapsed().as_secs() >= 30 || inventory.applications.len() >= 10_000 || pending.len() >= 10_000 {
        inventory.incomplete = true;
        break 'scan;
      }
      let Ok(child) = child else {
        inventory.incomplete = true;
        continue;
      };
      let path = child.path();
      let Ok(metadata) = fs::symlink_metadata(&path) else {
        inventory.incomplete = true;
        continue;
      };
      if is_bundle(&path) && (metadata.is_dir() || metadata.is_symlink()) {
        let linked = metadata.is_symlink() || fs::canonicalize(&path).is_ok_and(|canonical| canonical != path);
        let info = (!linked).then(|| read_info(&path)).flatten();
        let blocked = if origin == ApplicationOrigin::System || system_flags(&metadata) {
          Some(RemovalBlock::System)
        } else if linked {
          Some(RemovalBlock::SymbolicLink)
        } else if current.as_ref().is_some_and(|current| current.starts_with(&path)) {
          Some(RemovalBlock::CurrentApplication)
        } else if info.is_none() {
          Some(RemovalBlock::InvalidBundle)
        } else {
          None
        };
        let info_identity = info_path(&path).and_then(|path| fs::metadata(path).ok()).map(|metadata| identity(&metadata));
        let info = info.unwrap_or_default();
        let bytes = if linked { None } else { measure(&path, &mut size_budget, started) };
        inventory.applications.push(InstalledApplication {
          name: info.name.unwrap_or_else(|| path.file_stem().unwrap_or_default().to_string_lossy().into_owned()),
          path,
          version: info.version,
          bundle_id: info.bundle_id,
          bytes,
          origin,
          blocked,
          identity: (!linked).then(|| identity(&metadata)),
          info_identity,
        });
      } else if metadata.is_dir() && !metadata.is_symlink() && depth < 3 {
        pending.push((path, origin, depth + 1));
      }
    }
  }
  inventory.applications.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()).then_with(|| left.path.cmp(&right.path)));
  inventory.applications.dedup_by(|left, right| left.path == right.path);
  inventory
}

fn is_bundle(path: &Path) -> bool {
  path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
}

fn identity(metadata: &fs::Metadata) -> Identity {
  #[cfg(unix)]
  use std::os::unix::fs::MetadataExt;

  Identity {
    created: metadata.created().ok(),
    modified: metadata.modified().ok(),
    length: metadata.len(),
    #[cfg(unix)]
    device: metadata.dev(),
    #[cfg(unix)]
    inode: metadata.ino(),
  }
}

fn system_flags(metadata: &fs::Metadata) -> bool {
  #[cfg(target_os = "macos")]
  {
    use std::os::macos::fs::MetadataExt;

    // UF_IMMUTABLE, SF_IMMUTABLE and SF_RESTRICTED.
    metadata.st_flags() & (0x2 | 0x20000 | 0x80000) != 0
  }

  #[cfg(not(target_os = "macos"))]
  {
    let _ = metadata;
    false
  }
}

fn info_path(bundle: &Path) -> Option<PathBuf> {
  let path = bundle.join("Contents/Info.plist");
  let metadata = fs::symlink_metadata(&path).ok()?;
  (metadata.is_file() && !metadata.is_symlink() && metadata.len() <= 1024 * 1024 && fs::canonicalize(&path).ok()? == path).then_some(path)
}

fn parse_bundle_info(data: &[u8]) -> Option<BundleInfo> {
  let value: serde_json::Value = serde_json::from_slice(data).ok()?;
  let text = |key: &str| value.get(key).and_then(|value| value.as_str()).filter(|value| !value.trim().is_empty()).map(str::to_owned);
  let bundle_id = text("CFBundleIdentifier")?;
  Some(BundleInfo {
    name: text("CFBundleDisplayName").or_else(|| text("CFBundleName")),
    version: text("CFBundleShortVersionString").or_else(|| text("CFBundleVersion")),
    bundle_id: Some(bundle_id),
  })
}

fn read_bundle_info(bundle: &Path) -> Option<BundleInfo> {
  let path = info_path(bundle)?;
  let output = std::process::Command::new("/usr/bin/plutil").args(["-convert", "json", "-o", "-"]).arg(path).output().ok()?;
  output.status.success().then(|| parse_bundle_info(&output.stdout)).flatten()
}

fn measure(path: &Path, remaining: &mut usize, started: Instant) -> Option<u64> {
  let mut pending = vec![path.to_path_buf()];
  let mut total = 0_u64;
  #[cfg(unix)]
  let mut seen = HashSet::new();
  while let Some(path) = pending.pop() {
    if *remaining == 0 || started.elapsed().as_secs() >= 25 {
      return None;
    }
    *remaining -= 1;
    let metadata = fs::symlink_metadata(&path).ok()?;
    if metadata.is_symlink() {
      continue;
    }

    #[cfg(unix)]
    {
      use std::os::unix::fs::MetadataExt;

      if !seen.insert((metadata.dev(), metadata.ino())) {
        continue;
      }
      total = total.saturating_add(metadata.blocks().saturating_mul(512));
    }

    #[cfg(not(unix))]
    {
      total = total.saturating_add(metadata.len());
    }
    if metadata.is_dir() {
      for child in fs::read_dir(&path).ok()? {
        pending.push(child.ok()?.path());
        if pending.len() > *remaining {
          return None;
        }
      }
    }
  }
  Some(total)
}

impl InstalledApplication {
  fn validate(&self, home: &Path, current_exe: Option<&Path>) -> Result<(), String> {
    if self.blocked.is_some() || self.origin == ApplicationOrigin::System {
      return Err("This application is protected or cannot be removed by TendKit.".into());
    }
    if current_exe.is_some_and(|exe| exe.starts_with(&self.path)) {
      return Err("TendKit cannot remove the application it is running from.".into());
    }
    let allowed = roots(home).into_iter().filter(|(_, origin)| *origin != ApplicationOrigin::System).any(|(root, _)| {
      let Ok(relative) = self.path.strip_prefix(&root) else { return false };
      if relative.as_os_str().is_empty() || !is_bundle(&self.path) {
        return false;
      }
      let mut cursor = root.clone();
      for component in relative.components() {
        if !matches!(component, std::path::Component::Normal(_)) {
          return false;
        }
        cursor.push(component);
        if !fs::symlink_metadata(&cursor).is_ok_and(|metadata| metadata.is_dir() && !metadata.is_symlink() && !system_flags(&metadata)) {
          return false;
        }
        if cursor != self.path && is_bundle(&cursor) {
          return false;
        }
      }
      fs::canonicalize(&self.path).is_ok_and(|canonical| canonical == self.path)
    });
    if !allowed || info_path(&self.path).is_none() {
      return Err("The application path is no longer a supported application bundle.".into());
    }
    let metadata = fs::symlink_metadata(&self.path).map_err(|error| error.to_string())?;
    let current_info = info_path(&self.path).and_then(|path| fs::metadata(path).ok()).map(|metadata| identity(&metadata));
    if self.identity.as_ref() != Some(&identity(&metadata)) || self.info_identity != current_info {
      return Err("The application changed since the scan. Refresh the list before removing it.".into());
    }
    Ok(())
  }
}

/// Validate immediately before invoking a supplied native Trash backend.
pub fn move_to_trash(
  app: &InstalledApplication,
  home: &Path,
  current_exe: Option<&Path>,
  trash: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<(), String> {
  app.validate(home, current_exe)?;
  trash(&app.path)
}

#[cfg(test)]
#[path = "../tests/applications.rs"]
mod tests;
