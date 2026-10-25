//! Persistent, read-only disk usage index. Navigation never touches scanned files.

use serde::{Deserialize, Serialize};

#[cfg(unix)]
use std::collections::HashSet;

use std::{
  collections::HashMap,
  fs,
  path::{Path, PathBuf},
  process::Command,
  sync::atomic::{AtomicBool, Ordering},
  time::{Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiskEntry {
  pub path: PathBuf,
  pub bytes: u64,
  pub directory: bool,
  pub excluded: bool,
  pub measured: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DiskReport {
  pub entries: Vec<DiskEntry>,
  pub capacity: Option<(u64, u64, u64)>,
  pub skipped: usize,
  pub excluded: usize,
  pub partial: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiskIndex {
  version: u32,
  pub root: PathBuf,
  home: PathBuf,
  pub scanned_at: u64,
  reports: HashMap<PathBuf, DiskReport>,
}

impl DiskIndex {
  pub fn report(&self, path: &Path) -> Option<&DiskReport> {
    self.reports.get(path)
  }

  pub fn cache_path() -> PathBuf {
    crate::paths::support_dir().join("disk-index.json")
  }

  pub fn load(path: &Path, root: &Path) -> Option<Self> {
    if fs::metadata(path).ok()?.len() > 256 * 1024 * 1024 {
      return None;
    }
    let index: Self = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    let home = crate::paths::home_dir();
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    (index.scanned_at <= now.saturating_add(300)
      && index.version == 1
      && index.root == root
      && index.home == home
      && index.reports.contains_key(root))
    .then_some(index)
  }

  pub fn save(&self, path: &Path) -> Result<(), String> {
    let data = serde_json::to_vec(self).map_err(|error| error.to_string())?;
    if data.len() > 256 * 1024 * 1024 {
      return Err("Disk index exceeds the cache size limit.".into());
    }
    if let Some(parent) = path.parent() {
      fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    crate::fs_util::write_atomic(path, &data, 0o600).map_err(|error| error.to_string())
  }
}

struct Scan<'a> {
  cancel: &'a AtomicBool,
  home: PathBuf,
  started: Instant,
  visited: usize,
  #[cfg(unix)]
  seen: HashSet<(u64, u64)>,
  report: DiskReport,
  reports: HashMap<PathBuf, DiskReport>,
}

impl Scan<'_> {
  fn stopped(&mut self) -> bool {
    let stopped = self.cancel.load(Ordering::Relaxed) || self.visited >= 1_000_000 || self.started.elapsed().as_secs() >= 600;
    self.report.partial |= stopped;
    stopped
  }

  fn size(&mut self, path: &Path, depth: usize) -> u64 {
    if depth >= 128 || self.stopped() {
      self.report.partial = true;
      return 0;
    }
    if privacy_excluded(path, &self.home) {
      self.report.excluded += 1;
      return 0;
    }
    self.visited += 1;
    let Ok(metadata) = fs::symlink_metadata(path) else {
      self.report.skipped += 1;
      return 0;
    };
    if metadata.is_symlink() {
      return 0;
    }

    #[cfg(unix)]
    let own = {
      use std::os::unix::fs::MetadataExt;

      if !self.seen.insert((metadata.dev(), metadata.ino())) {
        return 0;
      }
      metadata.blocks().saturating_mul(512)
    };

    #[cfg(not(unix))]
    let own = metadata.len();
    if !metadata.is_dir() {
      return own;
    }
    let Ok(children) = fs::read_dir(path) else {
      self.report.skipped += 1;
      return own;
    };
    // List immediate children before descending so a large first subtree does
    // not hide the rest of the folder when the scan is cancelled or bounded.
    let mut entries = Vec::new();
    for child in children {
      match child {
        Ok(child) => {
          let path = child.path();
          let excluded = privacy_excluded(&path, &self.home);
          let directory = !excluded && child.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
          entries.push(DiskEntry { path, bytes: 0, directory, excluded, measured: false });
        }
        Err(_) => self.report.skipped += 1,
      }
      if entries.len() >= 100_000 || self.cancel.load(Ordering::Relaxed) {
        self.report.partial = true;
        break;
      }
    }
    entries.sort_by_key(|entry| !self.home.starts_with(&entry.path));
    let skipped_before = self.report.skipped;
    let excluded_before = self.report.excluded;
    let mut bytes = own;
    for entry in &mut entries {
      if entry.excluded {
        self.report.excluded += 1;
        continue;
      }
      if self.stopped() {
        continue;
      }
      entry.bytes = self.size(&entry.path, depth + 1);
      entry.measured = self.reports.contains_key(&entry.path) || !entry.directory;
      bytes = bytes.saturating_add(entry.bytes);
    }
    entries.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.path.cmp(&b.path)));
    self.reports.insert(
      path.to_path_buf(),
      DiskReport {
        entries,
        skipped: self.report.skipped - skipped_before,
        excluded: self.report.excluded - excluded_before,
        partial: self.report.partial,
        capacity: None,
      },
    );
    bytes
  }
}

pub fn disk_root() -> PathBuf {
  if cfg!(windows) {
    std::env::var_os("SystemDrive").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("C:")).join(std::path::MAIN_SEPARATOR.to_string())
  } else {
    PathBuf::from("/")
  }
}

pub fn index_disk(root: &Path, cancel: &AtomicBool) -> Result<DiskIndex, String> {
  index_with_home(root, cancel, crate::paths::home_dir())
}

fn index_with_home(root: &Path, cancel: &AtomicBool, home: PathBuf) -> Result<DiskIndex, String> {
  if privacy_excluded(root, &home) {
    return Err("This location is excluded from disk inspection to avoid privacy permission requests.".into());
  }
  // Validate the root before scanning so an unreadable root is an error.
  fs::read_dir(root).map_err(|error| format!("Cannot read {}: {error}", root.display()))?;
  let mut scan = Scan {
    cancel,
    home: home.clone(),
    started: Instant::now(),
    visited: 0,
    #[cfg(unix)]
    seen: HashSet::new(),
    report: DiskReport::default(),
    reports: HashMap::new(),
  };
  scan.size(root, 0);
  let capacity = filesystem_capacity(if home.as_os_str().is_empty() { root } else { &home });
  if !scan.reports.contains_key(root) {
    scan.reports.insert(root.to_path_buf(), DiskReport { partial: true, ..DiskReport::default() });
  }
  for report in scan.reports.values_mut() {
    report.capacity = capacity;
  }
  Ok(DiskIndex {
    version: 1,
    root: root.to_path_buf(),
    home,
    scanned_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
    reports: scan.reports,
  })
}

/// A single-folder projection, retained for callers that only need a report.
pub fn inspect_disk(path: &Path, cancel: &AtomicBool) -> Result<DiskReport, String> {
  inspect_with_home(path, cancel, crate::paths::home_dir())
}

fn inspect_with_home(path: &Path, cancel: &AtomicBool, home: PathBuf) -> Result<DiskReport, String> {
  index_with_home(path, cancel, home)?.report(path).cloned().ok_or_else(|| "Directory was not indexed.".into())
}

/// Conservative macOS policy: do not probe known privacy domains, mounted
/// providers, other users, or system data. This runs before metadata/read_dir.
fn privacy_excluded(path: &Path, home: &Path) -> bool {
  if cfg!(windows) {
    let root = disk_root();
    return ["Windows", "System Volume Information", "$Recycle.Bin", "pagefile.sys", "swapfile.sys", "hiberfil.sys"]
      .iter()
      .any(|name| path.starts_with(root.join(name)));
  }
  if cfg!(target_os = "linux") {
    return ["/proc", "/sys", "/dev", "/run", "/mnt", "/media", "/root"].iter().any(|root| path.starts_with(root));
  }
  if !cfg!(target_os = "macos") {
    return false;
  }
  if ["/System", "/Library", "/private", "/Volumes", "/Network", "/dev", "/.vol"].iter().any(|root| path.starts_with(root)) {
    return true;
  }
  if let Ok(relative) = path.strip_prefix("/Users") {
    if let Some(user) = relative.components().next() {
      if !home.starts_with(Path::new("/Users").join(user)) {
        return true;
      }
    }
  }
  if home.as_os_str().is_empty() {
    return false;
  }
  ["Desktop", "Documents", "Downloads", "Pictures", "Movies", "Music", "Library", ".Trash", "Applications", "Public"]
    .iter()
    .any(|name| path.starts_with(home.join(name)))
}

fn filesystem_capacity(path: &Path) -> Option<(u64, u64, u64)> {
  if cfg!(windows) {
    let drive = path.components().next()?.as_os_str().to_string_lossy().trim_end_matches('\\').to_string();
    let output = Command::new("powershell.exe").args(["-NoProfile", "-NonInteractive", "-Command", "$d = Get-CimInstance Win32_LogicalDisk | Where-Object DeviceID -eq $env:TENDKIT_DISK; if ($null -eq $d) { exit 1 }; Write-Output ($d.Size.ToString() + ' ' + ($d.Size - $d.FreeSpace).ToString() + ' ' + $d.FreeSpace.ToString())"])
      .env("TENDKIT_DISK", drive).output().ok()?;
    if !output.status.success() {
      return None;
    }
    let values: Vec<u64> = String::from_utf8_lossy(&output.stdout).split_whitespace().filter_map(|value| value.parse().ok()).collect();
    return (values.len() == 3).then(|| (values[0], values[1], values[2]));
  }
  let output = Command::new("/bin/df").args(["-k", "-P"]).arg(path).env("LC_ALL", "C").output().ok()?;
  if !output.status.success() {
    return None;
  }
  parse_capacity(&String::from_utf8_lossy(&output.stdout))
}

fn parse_capacity(output: &str) -> Option<(u64, u64, u64)> {
  let fields: Vec<_> = output.lines().last()?.split_whitespace().collect();
  let percent = fields.iter().position(|field| field.ends_with('%'))?;
  let value = |offset| fields.get(percent.checked_sub(offset)?)?.parse::<u64>().ok()?.checked_mul(1024);
  Some((value(3)?, value(2)?, value(1)?))
}

#[cfg(test)]
#[path = "../tests/disk.rs"]
mod tests;
