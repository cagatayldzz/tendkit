use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);

impl Fixture {
  fn new() -> Self {
    let path = std::env::temp_dir().join(format!("tendkit-cleanup-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    fs::create_dir_all(&path).unwrap();
    Self(fs::canonicalize(path).unwrap())
  }

  fn candidate(&self, relative: &str, kind: CleanupKind) -> CleanupCandidate {
    let path = self.0.join(relative);
    fs::create_dir_all(&path).unwrap();
    CleanupCandidate::new(path, 8192, kind)
  }

  fn rust_target(&self) -> CleanupCandidate {
    let candidate = self.candidate("projects/app/target", CleanupKind::RustTarget);
    fs::write(candidate.path.parent().unwrap().join("Cargo.toml"), b"[package]").unwrap();
    fs::write(candidate.path.join("CACHEDIR.TAG"), b"Signature: 8a477f597d28d172789f06886806bc55\n").unwrap();
    candidate
  }
}

impl Drop for Fixture {
  fn drop(&mut self) {
    fs::remove_dir_all(&self.0).unwrap();
  }
}

#[test]
fn only_cache_and_build_categories_are_recommended() {
  assert!(CleanupKind::BunCache.recommended());
  assert!(CleanupKind::RustTarget.recommended());
  assert!(!CleanupKind::NodeModules.recommended());
  assert!(!CleanupKind::PythonEnvironment.recommended());
}

#[test]
fn verified_cache_and_build_outputs_can_reach_the_injected_trash_backend() {
  let fixture = Fixture::new();
  let cache = fixture.candidate(".bun/install/cache", CleanupKind::BunCache);
  let rust = fixture.rust_target();
  let candidates = prepare(vec![cache, rust], &fixture.0);
  assert_eq!(candidates.len(), 2);
  for candidate in candidates {
    let mut called = false;
    move_to_trash(&candidate, &fixture.0, |path| {
      assert_eq!(path, candidate.path);
      called = true;
      Ok(())
    })
    .unwrap();
    assert!(called);
    assert!(candidate.path.exists());
  }
}

#[test]
fn missing_or_forged_project_markers_are_rejected() {
  let fixture = Fixture::new();
  let rust = fixture.candidate("projects/app/target", CleanupKind::RustTarget);
  let node = fixture.candidate("projects/node_modules", CleanupKind::NodeModules);
  let python = fixture.candidate("projects/.venv", CleanupKind::PythonEnvironment);
  assert!(prepare(vec![rust.clone(), node, python], &fixture.0).is_empty());
  fs::write(rust.path.parent().unwrap().join("Cargo.toml"), b"[package]").unwrap();
  fs::write(rust.path.join("CACHEDIR.TAG"), b"not a cache marker").unwrap();
  assert!(prepare(vec![rust], &fixture.0).is_empty());
}

#[test]
fn protected_personal_and_tool_directories_are_not_cleanup_candidates() {
  let fixture = Fixture::new();
  let mut candidates = Vec::new();
  for parent in ["Documents", "documents", "Desktop", "Downloads", "Library", ".local", ".rustup", ".Trash", "projects/.git"] {
    let candidate = fixture.candidate(&format!("{parent}/app/node_modules"), CleanupKind::NodeModules);
    fs::write(candidate.path.parent().unwrap().join("package.json"), b"{}").unwrap();
    candidates.push(candidate);
  }
  candidates.push(fixture.candidate(".bun/bin", CleanupKind::BunCache));
  assert!(prepare(candidates, &fixture.0).is_empty());
}

#[test]
fn nested_candidates_are_only_moved_once_and_environments_need_markers() {
  let fixture = Fixture::new();
  let outer = fixture.candidate("project/node_modules", CleanupKind::NodeModules);
  let inner = fixture.candidate("project/node_modules/dependency/node_modules", CleanupKind::NodeModules);
  for candidate in [&outer, &inner] {
    fs::write(candidate.path.parent().unwrap().join("package.json"), b"{}").unwrap();
  }
  let python = fixture.candidate("project/.venv", CleanupKind::PythonEnvironment);
  fs::write(python.path.join("pyvenv.cfg"), b"home = /example/python").unwrap();
  let candidates = prepare(vec![inner, outer.clone(), python], &fixture.0);
  assert_eq!(candidates.len(), 2);
  assert!(candidates.iter().any(|candidate| candidate.path == outer.path));
}

#[test]
fn replaced_directory_or_removed_marker_never_reaches_the_backend() {
  let fixture = Fixture::new();
  let rust = fixture.rust_target();
  let candidate = prepare(vec![rust], &fixture.0).pop().unwrap();
  fs::rename(&candidate.path, candidate.path.with_file_name("previous-target")).unwrap();
  fs::create_dir(&candidate.path).unwrap();
  fs::write(candidate.path.join("CACHEDIR.TAG"), b"Signature: 8a477f597d28d172789f06886806bc55\n").unwrap();
  let mut called = false;
  assert!(move_to_trash(&candidate, &fixture.0, |_| {
    called = true;
    Ok(())
  })
  .is_err());
  assert!(!called);
  let candidate = prepare(vec![candidate], &fixture.0).pop().unwrap();
  fs::remove_file(candidate.path.join("CACHEDIR.TAG")).unwrap();
  assert!(candidate.validate(&fixture.0).is_err());
}

#[cfg(unix)]
#[test]
fn symlinked_directories_ancestors_and_markers_are_rejected() {
  use std::os::unix::fs::symlink;

  let fixture = Fixture::new();
  let other = Fixture::new();
  let rust = fixture.rust_target();
  symlink(rust.path.parent().unwrap(), fixture.0.join("linked-project")).unwrap();
  let linked = CleanupCandidate::new(fixture.0.join("linked-project/target"), 1, CleanupKind::RustTarget);
  assert!(prepare(vec![linked], &fixture.0).is_empty());
  fs::remove_file(rust.path.join("CACHEDIR.TAG")).unwrap();
  fs::write(other.0.join("marker"), b"Signature: 8a477f597d28d172789f06886806bc55\n").unwrap();
  symlink(other.0.join("marker"), rust.path.join("CACHEDIR.TAG")).unwrap();
  assert!(prepare(vec![rust], &fixture.0).is_empty());
  let cache = fixture.candidate(".bun/install/cache", CleanupKind::BunCache);
  fs::remove_dir(&cache.path).unwrap();
  symlink(&other.0, &cache.path).unwrap();
  assert!(prepare(vec![cache], &fixture.0).is_empty());
}

#[test]
fn paths_outside_home_are_rejected_and_backend_failures_are_preserved() {
  let fixture = Fixture::new();
  let other = Fixture::new();
  let rust = fixture.rust_target();
  assert!(prepare(vec![rust.clone()], &other.0).is_empty());
  let candidate = prepare(vec![rust], &fixture.0).pop().unwrap();
  assert_eq!(move_to_trash(&candidate, &fixture.0, |_| Err("Trash unavailable".into())), Err("Trash unavailable".into()));
  assert!(candidate.path.exists());
}
