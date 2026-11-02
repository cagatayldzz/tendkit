//! Persistent, read-only disk usage index. Navigation never touches scanned files.

use serde::{Deserialize, Serialize};

#[cfg(unix)]
use std::collections::HashSet;

use std::{
  collections::HashMap,
  fs,
  path::{Path, PathBuf},
  process::Command,
  sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Mutex,
  },
  thread,
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
  /// Locations that could not be read, up to [`MAX_SKIPPED_PATHS`].
  #[serde(default)]
  pub skipped_paths: Vec<PathBuf>,
  pub excluded: usize,
  pub partial: bool,
}

/// Allocated space in recognized development directories. Nested matches are
/// counted only as part of their outermost recognized directory.
#[derive(Clone, Debug, Default)]
pub struct DeveloperUsage {
  pub bytes: [u64; 3],
  pub counts: [usize; 3],
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

  /// Classify names using the saved index only; no filesystem probes are made.
  pub fn developer_kind(&self, entry: &DiskEntry) -> Option<usize> {
    if !entry.directory || entry.excluded || !entry.measured {
      return None;
    }
    match entry.path.file_name()?.to_str()? {
      "node_modules" => Some(0),
      "target"
        if self
          .report(entry.path.parent()?)?
          .entries
          .iter()
          .any(|sibling| sibling.path.file_name().is_some_and(|name| name == "Cargo.toml") && !sibling.directory) =>
      {
        Some(1)
      }
      ".venv" => Some(2),
      _ => None,
    }
  }

  pub fn developer_usage(&self, path: &Path) -> DeveloperUsage {
    let mut usage = DeveloperUsage::default();
    let matches: HashMap<_, _> = self
      .reports
      .values()
      .flat_map(|report| &report.entries)
      .filter(|entry| entry.path.starts_with(path))
      .filter_map(|entry| self.developer_kind(entry).map(|kind| (entry.path.as_path(), (kind, entry.bytes))))
      .collect();
    for (entry_path, (kind, bytes)) in &matches {
      let nested = entry_path.ancestors().skip(1).take_while(|ancestor| ancestor.starts_with(path)).any(|ancestor| matches.contains_key(ancestor));
      if !nested {
        usage.bytes[*kind] = usage.bytes[*kind].saturating_add(*bytes);
        usage.counts[*kind] += 1;
      }
    }
    usage
  }

  /// Proposals from indexed locations only. Live validation is required before use.
  pub fn cleanup_proposals(&self) -> Vec<crate::cleanup::CleanupCandidate> {
    let mut proposals = Vec::new();
    for report in self.reports.values() {
      for entry in &report.entries {
        if !entry.directory || !entry.measured || entry.excluded || entry.bytes == 0 || !entry.path.starts_with(&self.home) {
          continue;
        }
        let kind = if entry.path == self.home.join(".bun/install/cache") {
          Some(crate::cleanup::CleanupKind::BunCache)
        } else {
          self.developer_kind(entry).map(|kind| match kind {
            0 => crate::cleanup::CleanupKind::NodeModules,
            1 => crate::cleanup::CleanupKind::RustTarget,
            _ => crate::cleanup::CleanupKind::PythonEnvironment,
          })
        };
        if let Some(kind) = kind {
          proposals.push(crate::cleanup::CleanupCandidate::new(entry.path.clone(), entry.bytes, kind));
        }
      }
    }
    proposals
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

/// Subtree totals returned by [`Scan::size`] and merged into the parent.
/// How many unreadable locations a report names; the count stays exact.
pub const MAX_SKIPPED_PATHS: usize = 50;

#[derive(Default)]
struct Totals {
  bytes: u64,
  skipped: usize,
  skipped_paths: Vec<PathBuf>,
  excluded: usize,
  partial: bool,
}

impl Totals {
  fn skip(&mut self, path: &Path) {
    self.skipped += 1;
    if self.skipped_paths.len() < MAX_SKIPPED_PATHS {
      self.skipped_paths.push(path.to_path_buf());
    }
  }

  fn merge(&mut self, other: Totals) {
    self.bytes = self.bytes.saturating_add(other.bytes);
    self.skipped += other.skipped;
    let room = MAX_SKIPPED_PATHS - self.skipped_paths.len();
    self.skipped_paths.extend(other.skipped_paths.into_iter().take(room));
    self.excluded += other.excluded;
    self.partial |= other.partial;
  }
}

/// Upper bound on concurrently running scan threads, including the caller.
const MAX_SCAN_THREADS: usize = 8;

/// Stack for scan threads; recursion is bounded at 128 levels.
const SCAN_STACK_BYTES: usize = 8 * 1024 * 1024;

struct Scan<'a> {
  cancel: &'a AtomicBool,
  home: PathBuf,
  started: Instant,
  visited: AtomicUsize,
  /// Extra threads currently running besides the caller.
  workers: AtomicUsize,
  worker_limit: usize,
  #[cfg(unix)]
  seen: Mutex<HashSet<(u64, u64)>>,
  reports: Mutex<HashMap<PathBuf, DiskReport>>,
}

impl Scan<'_> {
  fn stopped(&self) -> bool {
    self.cancel.load(Ordering::Relaxed) || self.visited.load(Ordering::Relaxed) >= 1_000_000 || self.started.elapsed().as_secs() >= 600
  }

  fn try_acquire_worker(&self) -> bool {
    let mut active = self.workers.load(Ordering::Acquire);
    while active < self.worker_limit {
      match self.workers.compare_exchange_weak(active, active + 1, Ordering::AcqRel, Ordering::Acquire) {
        Ok(_) => return true,
        Err(current) => active = current,
      }
    }
    false
  }

  fn release_worker(&self) {
    self.workers.fetch_sub(1, Ordering::AcqRel);
  }

  fn size(&self, path: &Path, depth: usize) -> Totals {
    let mut totals = Totals::default();
    if depth >= 128 || self.stopped() {
      totals.partial = true;
      return totals;
    }
    if privacy_excluded(path, &self.home) {
      totals.excluded = 1;
      return totals;
    }
    self.visited.fetch_add(1, Ordering::Relaxed);
    let Ok(metadata) = fs::symlink_metadata(path) else {
      totals.skip(path);
      return totals;
    };
    if metadata.is_symlink() {
      return totals;
    }

    #[cfg(unix)]
    let own = {
      use std::os::unix::fs::MetadataExt;

      if !self.seen.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).insert((metadata.dev(), metadata.ino())) {
        return totals;
      }
      metadata.blocks().saturating_mul(512)
    };

    #[cfg(not(unix))]
    let own = metadata.len();
    totals.bytes = own;
    if !metadata.is_dir() {
      return totals;
    }
    let Ok(children) = fs::read_dir(path) else {
      totals.skip(path);
      return totals;
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
        Err(_) => totals.skip(path),
      }
      if entries.len() >= 100_000 || self.cancel.load(Ordering::Relaxed) {
        totals.partial = true;
        break;
      }
    }
    entries.sort_by_key(|entry| !self.home.starts_with(&entry.path));

    // Directories run on spare worker threads while any are free; files and
    // the remainder are measured inline. Results are merged in entry order.
    let mut results: Vec<Option<Totals>> = entries.iter().map(|_| None).collect();
    thread::scope(|scope| {
      let mut handles = Vec::new();
      for (index, entry) in entries.iter().enumerate() {
        if entry.excluded {
          totals.excluded += 1;
          continue;
        }
        if self.stopped() {
          totals.partial = true;
          continue;
        }
        if entry.directory && self.try_acquire_worker() {
          let spawned = thread::Builder::new().stack_size(SCAN_STACK_BYTES).spawn_scoped(scope, move || {
            let result = self.size(&entry.path, depth + 1);
            self.release_worker();
            result
          });
          match spawned {
            Ok(handle) => handles.push((index, handle)),
            Err(_) => {
              self.release_worker();
              results[index] = Some(self.size(&entry.path, depth + 1));
            }
          }
        } else {
          results[index] = Some(self.size(&entry.path, depth + 1));
        }
      }
      for (index, handle) in handles {
        results[index] = Some(handle.join().unwrap_or_else(|_| {
          self.release_worker();
          Totals { partial: true, ..Totals::default() }
        }));
      }
    });

    {
      let reports = self.reports.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
      for (entry, result) in entries.iter_mut().zip(results) {
        let Some(result) = result else { continue };
        entry.bytes = result.bytes;
        entry.measured = reports.contains_key(&entry.path) || !entry.directory;
        totals.merge(result);
      }
    }
    entries.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.path.cmp(&b.path)));
    self.reports.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).insert(
      path.to_path_buf(),
      DiskReport {
        entries,
        skipped: totals.skipped,
        skipped_paths: totals.skipped_paths.clone(),
        excluded: totals.excluded,
        partial: totals.partial,
        capacity: None,
      },
    );
    totals
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
  let scan = Scan {
    cancel,
    home: home.clone(),
    started: Instant::now(),
    visited: AtomicUsize::new(0),
    workers: AtomicUsize::new(0),
    worker_limit: thread::available_parallelism().map_or(1, |count| count.get()).min(MAX_SCAN_THREADS) - 1,
    #[cfg(unix)]
    seen: Mutex::new(HashSet::new()),
    reports: Mutex::new(HashMap::new()),
  };
  scan.size(root, 0);
  let mut reports = scan.reports.into_inner().unwrap_or_else(|poisoned| poisoned.into_inner());
  let capacity = filesystem_capacity(if home.as_os_str().is_empty() { root } else { &home });
  if !reports.contains_key(root) {
    reports.insert(root.to_path_buf(), DiskReport { partial: true, ..DiskReport::default() });
  }
  for report in reports.values_mut() {
    report.capacity = capacity;
  }
  Ok(DiskIndex {
    version: 1,
    root: root.to_path_buf(),
    home,
    scanned_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
    reports,
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
