use super::*;
#[test]
fn block_is_idempotent_and_keeps_user_content() {
  let paths = vec!["/Users/test user's/.bun/bin".into(), "/opt/homebrew/bin".into()];
  let original = "# my config\nexport CUSTOM=yes\n";
  let once = render(original, &paths).unwrap();
  assert!(once.starts_with(original));
  assert!(once.contains("'\\''"));
  assert_eq!(render(&once, &paths).unwrap(), once);
  let updated = render(&once, &["/new/bin".into()]).unwrap();
  assert!(updated.starts_with(original));
  assert_eq!(updated.matches(BEGIN).count(), 1);
}
#[test]
fn malformed_blocks_are_rejected() {
  assert!(render(BEGIN, &[]).is_err());
  assert!(render(&format!("{END}\n{BEGIN}"), &[]).is_err());
  assert!(render(&format!("{BEGIN}\n{BEGIN}\n{END}"), &[]).is_err());
}
#[test]
fn writes_backup_and_refuses_symlinks() {
  let dir = std::env::temp_dir().join(format!("tendkit-shell-{}", std::process::id()));
  fs::create_dir_all(&dir).unwrap();
  let file = dir.join(".zshrc");
  let backups = dir.join("backups");
  fs::write(&file, "# personal\n").unwrap();
  assert!(update(&file, &["/test/bin".into()], &backups).unwrap());
  assert!(!update(&file, &["/test/bin".into()], &backups).unwrap());
  assert_eq!(fs::read_dir(&dir).unwrap().count(), 2);
  let saved: Vec<_> = fs::read_dir(&backups).unwrap().flatten().collect();
  assert_eq!(saved.len(), 1);
  assert_eq!(fs::read_to_string(saved[0].path()).unwrap(), "# personal\n");
  let link = dir.join("link");
  std::os::unix::fs::symlink(&file, &link).unwrap();
  assert!(update(&link, &[], &backups).is_err());
  fs::remove_dir_all(dir).unwrap();
}
#[test]
fn sourcing_twice_preserves_priority_without_expanding_path_text() {
  let dir = std::env::temp_dir().join(format!("tendkit-shell-eval-{}", std::process::id()));
  fs::create_dir_all(&dir).unwrap();
  let marker = dir.join("unexpected");
  let injected = format!("{}/user's/$(touch {})", dir.display(), marker.display());
  let file = dir.join("paths.zsh");
  fs::write(&file, render("", std::slice::from_ref(&injected)).unwrap()).unwrap();
  let output = std::process::Command::new("/bin/zsh")
    .args(["-f", "-c", "PATH=/usr/bin:/bin; source \"$1\"; source \"$1\"; print -r -- \"$PATH\"", "tendkit-test"])
    .arg(&file)
    .output()
    .unwrap();
  assert!(output.status.success());
  let value = String::from_utf8(output.stdout).unwrap();
  assert!(value.starts_with("/usr/bin:/bin:"));
  assert_eq!(value.matches(&injected).count(), 1);
  assert!(!marker.exists());
  fs::remove_dir_all(dir).unwrap();
}
