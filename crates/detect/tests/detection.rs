use super::*;

#[test]
fn parses_actual_cli_formats() {
  for (raw, expected) in [
    ("Homebrew 7.0.9\n", "7.0.9"),
    ("v26.11.1", "26.11.1"),
    ("Python 3.14.0\n", "3.14.0"),
    ("codex-cli 0.162.1", "0.162.1"),
    ("2.1.300 (Claude Code)", "2.1.300"),
    ("1.4.3-canary.12", "1.4.3-canary.12"),
  ] {
    assert_eq!(parse_version(raw).as_deref(), Some(expected));
  }
  assert_eq!(parse_version("command not found"), None);
  assert_eq!(parse_version("1.2"), None);
}

#[test]
fn detects_rustup_hard_link_proxies_in_custom_cargo_home() {
  let root = env::temp_dir().join(format!("tendkit-rustup-{}", std::process::id()));
  fs::create_dir_all(&root).unwrap();
  let rustup = root.join("rustup");
  let rustc = root.join("rustc");
  fs::write(&rustup, "#!/bin/sh\nprintf 'rustc 1.90.0 (fixture 2025-09-18)\\n'\n").unwrap();
  fs::set_permissions(&rustup, fs::Permissions::from_mode(0o700)).unwrap();
  fs::hard_link(&rustup, &rustc).unwrap();
  let installations = detect(tool("rust").unwrap(), std::slice::from_ref(&root));
  assert_eq!(installations.len(), 1);
  assert_eq!(installations[0].source, "rustup");
  assert_eq!(installations[0].version.as_deref(), Some("1.90.0"));
  fs::remove_dir_all(root).unwrap();
}

#[test]
fn recognizes_python_installation_sources() {
  assert_eq!(source(Path::new("/usr/bin/python3")), "System");
  assert_eq!(source(Path::new("/fixture/.pyenv/versions/3.14.0/bin/python3")), "pyenv");
  assert_eq!(source(Path::new("/Library/Frameworks/Python.framework/Versions/3.14/bin/python3")), "Python.org");
  assert_eq!(source(Path::new("/fixture/Cellar/python@3.14/3.14.0/bin/python3")), "Homebrew");
}

#[test]
fn detects_symlinks_once_and_preserves_unreadable_versions() {
  let root = env::temp_dir().join(format!("tendkit-test-{}", std::process::id()));
  fs::create_dir_all(&root).unwrap();
  let binary = root.join("fixture");
  fs::write(&binary, "#!/bin/sh\nprintf 'fixture 1.2.3\\n'\n").unwrap();
  fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
  let alias = root.join("alias");
  std::os::unix::fs::symlink(&binary, &alias).unwrap();
  let tool = Tool { id: "fixture", title: "Fixture", binaries: &["fixture", "alias"] };
  let detected = detect(&tool, std::slice::from_ref(&root));
  assert_eq!(detected.len(), 1);
  assert_eq!(detected[0].version.as_deref(), Some("1.2.3"));
  fs::write(&binary, "#!/bin/sh\nexit 3\n").unwrap();
  let broken = detect(&tool, std::slice::from_ref(&root));
  assert_eq!(broken.len(), 1);
  assert!(broken[0].version.is_none());
  assert!(broken[0].error.is_some());
  fs::remove_dir_all(root).unwrap();
}

#[test]
fn probes_run_concurrently_and_keep_search_order() {
  let root = env::temp_dir().join(format!("tendkit-parallel-{}", std::process::id()));
  let dirs: Vec<_> = (0..3).map(|i| root.join(i.to_string())).collect();
  for (i, dir) in dirs.iter().enumerate() {
    fs::create_dir_all(dir).unwrap();
    let binary = dir.join("slow");
    fs::write(&binary, format!("#!/bin/sh\n/bin/sleep 0.5\nprintf '1.0.{i}\\n'\n")).unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
  }
  let tool = Tool { id: "slow", title: "Slow", binaries: &["slow"] };
  let start = Instant::now();
  let detected = detect(&tool, &dirs);
  // Sequential probes would take at least 1.5 s.
  assert!(start.elapsed() < Duration::from_millis(1250));
  assert_eq!(detected.iter().map(|i| i.version.as_deref().unwrap()).collect::<Vec<_>>(), ["1.0.0", "1.0.1", "1.0.2"]);
  fs::remove_dir_all(root).unwrap();
}

#[test]
fn hanging_probe_times_out() {
  let root = env::temp_dir().join(format!("tendkit-timeout-{}", std::process::id()));
  fs::create_dir_all(&root).unwrap();
  let binary = root.join("hang");
  fs::write(&binary, "#!/bin/sh\n/bin/sleep 30 &\nwait\n").unwrap();
  fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
  let start = Instant::now();
  let error = version_within(&binary, &[], Duration::from_millis(300)).unwrap_err();
  assert!(error.contains("timed out"));
  assert!(start.elapsed() < Duration::from_secs(2));
  fs::remove_dir_all(root).unwrap();
}
