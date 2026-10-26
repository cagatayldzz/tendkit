use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);

impl Fixture {
  fn new() -> Self {
    let path = std::env::temp_dir().join(format!("tendkit-applications-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    fs::create_dir_all(path.join("Applications")).unwrap();
    Self(fs::canonicalize(path).unwrap())
  }

  fn bundle(&self, name: &str) -> PathBuf {
    let path = self.0.join("Applications").join(name);
    fs::create_dir_all(path.join("Contents")).unwrap();
    fs::write(
      path.join("Contents/Info.plist"),
      br#"{"CFBundleName":"Example App","CFBundleIdentifier":"org.example.app","CFBundleShortVersionString":"1.2.3"}"#,
    )
    .unwrap();
    path
  }

  fn inventory(&self, origin: ApplicationOrigin) -> ApplicationInventory {
    scan_roots(&[(self.0.join("Applications"), origin)], |path| parse_bundle_info(&fs::read(info_path(path)?).ok()?))
  }
}

impl Drop for Fixture {
  fn drop(&mut self) {
    fs::remove_dir_all(&self.0).unwrap();
  }
}

#[test]
fn bundle_metadata_uses_display_name_and_falls_back_to_build_version() {
  let info = parse_bundle_info(
    br#"{"CFBundleDisplayName":"Display Name","CFBundleName":"Bundle Name","CFBundleIdentifier":"org.example.app","CFBundleVersion":"42"}"#,
  )
  .unwrap();
  assert_eq!(info.name.as_deref(), Some("Display Name"));
  assert_eq!(info.version.as_deref(), Some("42"));
  assert!(parse_bundle_info(br#"{"CFBundleName":"Missing identifier"}"#).is_none());
  assert!(parse_bundle_info(b"invalid").is_none());
}

#[test]
fn scan_finds_nested_apps_but_not_internal_helpers() {
  let fixture = Fixture::new();
  let outer = fixture.bundle("Example.app");
  fixture.bundle("Utilities/Second.app");
  fs::create_dir_all(outer.join("Contents/Helpers/Internal.app")).unwrap();
  fs::write(outer.join("Contents/data"), vec![1; 8192]).unwrap();
  let inventory = fixture.inventory(ApplicationOrigin::User);
  assert_eq!(inventory.applications.len(), 2);
  let app = inventory.applications.iter().find(|app| app.path == outer).unwrap();
  assert_eq!(app.name, "Example App");
  assert_eq!(app.version.as_deref(), Some("1.2.3"));
  assert_eq!(app.blocked, None);
  assert!(app.bytes.unwrap() >= 8192);
}

#[test]
fn system_and_invalid_bundles_never_reach_the_trash_backend() {
  let fixture = Fixture::new();
  fixture.bundle("Protected.app");
  let app = fixture.inventory(ApplicationOrigin::System).applications.pop().unwrap();
  assert_eq!(app.blocked, Some(RemovalBlock::System));
  let mut called = false;
  assert!(move_to_trash(&app, &fixture.0, None, |_| {
    called = true;
    Ok(())
  })
  .is_err());
  assert!(!called);
  fs::create_dir(fixture.0.join("Applications/Invalid.app")).unwrap();
  let inventory = fixture.inventory(ApplicationOrigin::User);
  assert_eq!(inventory.applications.iter().find(|app| app.path.ends_with("Invalid.app")).unwrap().blocked, Some(RemovalBlock::InvalidBundle));
}

#[test]
fn removal_only_sends_the_reviewed_bundle_to_the_injected_backend() {
  let fixture = Fixture::new();
  let path = fixture.bundle("Example.app");
  let app = fixture.inventory(ApplicationOrigin::User).applications.pop().unwrap();
  let mut called = false;
  move_to_trash(&app, &fixture.0, None, |received| {
    called = true;
    assert_eq!(received, path);
    Ok(())
  })
  .unwrap();
  assert!(called);
  assert!(path.exists());
  assert_eq!(move_to_trash(&app, &fixture.0, None, |_| Err("Trash unavailable".into())), Err("Trash unavailable".into()));
}

#[test]
fn current_application_and_paths_outside_application_roots_are_rejected() {
  let fixture = Fixture::new();
  let path = fixture.bundle("Example.app");
  let app = fixture.inventory(ApplicationOrigin::User).applications.pop().unwrap();
  let mut called = false;
  assert!(move_to_trash(&app, &fixture.0, Some(&path.join("Contents/MacOS/example")), |_| {
    called = true;
    Ok(())
  })
  .is_err());
  assert!(!called);
  let mut outside = app;
  outside.path = fixture.0.join("Documents/Example.app");
  assert!(move_to_trash(&outside, &fixture.0, None, |_| {
    called = true;
    Ok(())
  })
  .is_err());
  assert!(!called);
}

#[test]
fn replaced_bundles_and_missing_metadata_are_rejected() {
  let fixture = Fixture::new();
  let path = fixture.bundle("Example.app");
  let app = fixture.inventory(ApplicationOrigin::User).applications.pop().unwrap();
  fs::rename(&path, path.with_file_name("Previous.app")).unwrap();
  fixture.bundle("Example.app");
  assert!(move_to_trash(&app, &fixture.0, None, |_| panic!("A changed bundle reached the Trash backend")).is_err());
  let fresh = fixture.inventory(ApplicationOrigin::User).applications.into_iter().find(|app| app.path == path).unwrap();
  fs::remove_file(path.join("Contents/Info.plist")).unwrap();
  assert!(move_to_trash(&fresh, &fixture.0, None, |_| panic!("An invalid bundle reached the Trash backend")).is_err());
}

#[cfg(unix)]
#[test]
fn symlinked_apps_and_ancestors_are_not_removed_and_size_probes_do_not_follow_links() {
  use std::os::unix::fs::symlink;

  let fixture = Fixture::new();
  let path = fixture.bundle("Example.app");
  symlink(&fixture.0, path.join("Contents/loop")).unwrap();
  symlink(&path, fixture.0.join("Applications/Alias.app")).unwrap();
  let inventory = fixture.inventory(ApplicationOrigin::User);
  let alias = inventory.applications.iter().find(|app| app.path.ends_with("Alias.app")).unwrap();
  assert_eq!(alias.blocked, Some(RemovalBlock::SymbolicLink));
  let mut called = false;
  assert!(move_to_trash(alias, &fixture.0, None, |_| {
    called = true;
    Ok(())
  })
  .is_err());
  assert!(!called);
  let app = inventory.applications.iter().find(|app| app.path == path).unwrap();
  assert!(app.bytes.is_some());
  fs::rename(fixture.0.join("Applications"), fixture.0.join("OriginalApplications")).unwrap();
  symlink(fixture.0.join("OriginalApplications"), fixture.0.join("Applications")).unwrap();
  assert!(move_to_trash(app, &fixture.0, None, |_| {
    called = true;
    Ok(())
  })
  .is_err());
  assert!(!called);
}

#[test]
fn size_limits_produce_unknown_size_instead_of_a_misleading_total() {
  let fixture = Fixture::new();
  let path = fixture.bundle("Example.app");
  assert_eq!(measure(&path, &mut 0, Instant::now()), None);
}
