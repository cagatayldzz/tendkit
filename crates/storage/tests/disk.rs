use super::*;
use std::sync::atomic::AtomicU64;

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);

impl Fixture {
  fn new() -> Self {
    let path = std::env::temp_dir().join(format!("tendkit-disk-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    fs::create_dir_all(&path).unwrap();
    Self(path)
  }
}

impl Drop for Fixture {
  fn drop(&mut self) {
    fs::remove_dir_all(&self.0).unwrap();
  }
}

#[test]
fn nested_files_are_sorted_and_symlinks_are_not_followed() {
  let fixture = Fixture::new();
  fs::create_dir(fixture.0.join("nested")).unwrap();
  fs::write(fixture.0.join("nested/data"), vec![1; 16384]).unwrap();
  fs::write(fixture.0.join("small"), b"small").unwrap();
  #[cfg(unix)]
  std::os::unix::fs::symlink(&fixture.0, fixture.0.join("loop")).unwrap();
  let report = inspect_disk(&fixture.0, &AtomicBool::new(false)).unwrap();
  assert!(!report.partial);
  assert_eq!(report.skipped, 0);
  assert_eq!(report.entries[0].path, fixture.0.join("nested"));
  assert!(report.entries[0].bytes >= 16384);
  #[cfg(unix)]
  assert_eq!(report.entries.iter().find(|entry| entry.path.ends_with("loop")).unwrap().bytes, 0);
}

#[test]
fn cancelled_scan_is_marked_partial() {
  let fixture = Fixture::new();
  fs::write(fixture.0.join("data"), b"data").unwrap();
  let report = inspect_disk(&fixture.0, &AtomicBool::new(true)).unwrap();
  assert!(report.partial);
  assert!(report.entries.is_empty());
}

#[test]
fn missing_directory_returns_error() {
  let fixture = Fixture::new();
  assert!(inspect_disk(&fixture.0.join("missing"), &AtomicBool::new(false)).is_err());
}

#[test]
fn capacity_parser_handles_spaces_and_invalid_output() {
  assert_eq!(
    parse_capacity("Filesystem 1024-blocks Used Available Capacity Mounted on\nMy Disk 1000 600 400 60% /My Disk\n"),
    Some((1024000, 614400, 409600))
  );
  assert_eq!(parse_capacity("unavailable"), None);
}

#[cfg(unix)]
#[test]
fn hard_links_are_counted_once() {
  use std::os::unix::fs::MetadataExt;
  let fixture = Fixture::new();
  let path = fixture.0.join("original");
  fs::write(&path, vec![1; 8192]).unwrap();
  fs::hard_link(&path, fixture.0.join("linked")).unwrap();
  let report = inspect_disk(&fixture.0, &AtomicBool::new(false)).unwrap();
  assert_eq!(report.entries.iter().map(|entry| entry.bytes).sum::<u64>(), fs::metadata(path).unwrap().blocks() * 512);
}

#[test]
fn deeply_nested_scan_is_marked_partial() {
  let fixture = Fixture::new();
  let mut path = fixture.0.clone();
  for _ in 0..130 {
    path.push("a");
    fs::create_dir(&path).unwrap();
  }
  fs::write(path.join("data"), b"data").unwrap();
  let report = inspect_disk(&fixture.0, &AtomicBool::new(false)).unwrap();
  assert!(report.partial);
}

#[cfg(target_os = "macos")]
#[test]
fn protected_locations_are_excluded_before_traversal() {
  let fixture = Fixture::new();
  fs::create_dir(fixture.0.join("Documents")).unwrap();
  fs::write(fixture.0.join("Documents/private"), vec![1; 8192]).unwrap();
  fs::create_dir(fixture.0.join("Projects")).unwrap();
  fs::write(fixture.0.join("Projects/data"), vec![1; 4096]).unwrap();
  let cancel = AtomicBool::new(false);
  let report = inspect_with_home(&fixture.0, &cancel, fixture.0.clone()).unwrap();
  assert_eq!(report.excluded, 1);
  let protected = report.entries.iter().find(|entry| entry.path.ends_with("Documents")).unwrap();
  assert!(protected.excluded);
  assert_eq!(protected.bytes, 0);
  assert!(report.entries.iter().find(|entry| entry.path.ends_with("Projects")).unwrap().bytes >= 4096);
  // A direct request is rejected before even reading the protected directory.
  assert!(inspect_with_home(&fixture.0.join("Documents"), &cancel, fixture.0.clone()).is_err());
}

#[cfg(target_os = "macos")]
#[test]
fn privacy_policy_covers_domains_and_respects_path_boundaries() {
  let home = Path::new("/Users/alex");
  for path in [
    "/Users/alex/Desktop/file",
    "/Users/alex/Downloads",
    "/Users/alex/Library/Mail",
    "/Users/other",
    "/Volumes/Disk",
    "/System/Volumes/Data",
    "/private/var",
  ] {
    assert!(privacy_excluded(Path::new(path), home), "{path}");
  }
  for path in ["/", "/Users", "/Users/alex", "/Users/alex/Projects", "/Users/alex/DesktopProject", "/Users/alex/.cargo", "/Applications"] {
    assert!(!privacy_excluded(Path::new(path), home), "{path}");
  }
}

#[test]
fn index_navigation_works_after_source_files_are_removed() {
  let fixture = Fixture::new();
  let folder = fixture.0.join("nested");
  fs::create_dir(&folder).unwrap();
  fs::write(folder.join("data"), vec![1; 8192]).unwrap();
  let index = index_disk(&fixture.0, &AtomicBool::new(false)).unwrap();
  fs::remove_dir_all(&folder).unwrap();
  let report = index.report(&folder).unwrap();
  assert_eq!(report.entries.len(), 1);
  assert!(report.entries[0].bytes >= 8192);
  assert!(report.entries[0].measured);
  assert!(index.report(&fixture.0).unwrap().entries.iter().any(|entry| entry.path == folder));
}

#[test]
fn saved_index_round_trips_and_rejects_invalid_cache() {
  let fixture = Fixture::new();
  fs::create_dir(fixture.0.join("nested")).unwrap();
  fs::write(fixture.0.join("nested/data"), vec![1; 4096]).unwrap();
  let index = index_disk(&fixture.0, &AtomicBool::new(false)).unwrap();
  let cache = fixture.0.join("index.json");
  index.save(&cache).unwrap();
  let loaded = DiskIndex::load(&cache, &fixture.0).unwrap();
  assert_eq!(loaded.scanned_at, index.scanned_at);
  assert_eq!(loaded.report(&fixture.0.join("nested")).unwrap().entries[0].bytes, index.report(&fixture.0.join("nested")).unwrap().entries[0].bytes);
  assert!(DiskIndex::load(&cache, Path::new("/different-root")).is_none());
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;

    assert_eq!(fs::metadata(&cache).unwrap().permissions().mode() & 0o777, 0o600);
  }
  let mut invalid = index.clone();
  invalid.version = 999;
  invalid.save(&cache).unwrap();
  assert!(DiskIndex::load(&cache, &fixture.0).is_none());
  invalid.version = 1;
  invalid.scanned_at = u64::MAX;
  invalid.save(&cache).unwrap();
  assert!(DiskIndex::load(&cache, &fixture.0).is_none());
  fs::write(&cache, b"invalid json").unwrap();
  assert!(DiskIndex::load(&cache, &fixture.0).is_none());
}

#[test]
fn index_contains_empty_directories_and_marks_unvisited_children() {
  let fixture = Fixture::new();
  let nested = fixture.0.join("empty");
  fs::create_dir(&nested).unwrap();
  let index = index_disk(&fixture.0, &AtomicBool::new(false)).unwrap();
  assert!(index.report(&nested).unwrap().entries.is_empty());
  assert!(index.report(&fixture.0).unwrap().entries[0].measured);
}
