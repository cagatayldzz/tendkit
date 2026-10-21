use std::{
  fs::{self, OpenOptions},
  io::Write,
  os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
  path::Path,
  time::{SystemTime, UNIX_EPOCH},
};

const BEGIN: &str = "# >>> TendKit >>>";
const END: &str = "# <<< TendKit <<<";

pub fn quote(value: &str) -> String {
  format!("'{}'", value.replace('\'', "'\\''"))
}

pub fn render(original: &str, paths: &[String]) -> Result<String, String> {
  if original.matches(BEGIN).count() > 1 || original.matches(END).count() > 1 {
    return Err("Multiple TendKit PATH blocks were found. Check the file.".into());
  }
  let mut block = format!("{BEGIN}\n");
  for path in paths {
    block.push_str(&format!("case \":$PATH:\" in\n  *{}*) ;;\n  *) export PATH=\"$PATH\":{} ;;\nesac\n", quote(&format!(":{path}:")), quote(path)));
  }
  block.push_str(END);
  let mut result = original.to_owned();
  match (original.find(BEGIN), original.find(END)) {
    (Some(start), Some(end))
      if start < end
        && (start == 0 || original.as_bytes()[start - 1] == b'\n')
        && (end + END.len() == original.len() || original.as_bytes()[end + END.len()] == b'\n') =>
    {
      result.replace_range(start..end + END.len(), &block);
    }
    (None, None) => {
      if !result.is_empty() && !result.ends_with('\n') {
        result.push('\n');
      }
      result.push_str(&block);
      result.push('\n');
    }
    _ => return Err("The TendKit PATH block is incomplete or malformed. Check the file.".into()),
  }
  Ok(result)
}

/// Backups go to `backups` (created with 0700) rather than beside the file, so
/// repeated updates do not clutter the home directory.
pub fn update(file: &Path, paths: &[String], backups: &Path) -> Result<bool, String> {
  if let Ok(meta) = fs::symlink_metadata(file) {
    if meta.file_type().is_symlink() || !meta.is_file() {
      return Err(format!("{} is not a regular file. No automatic PATH changes were applied.", file.display()));
    }
    if meta.len() > 1_048_576 {
      return Err("Shell configuration file is too large".into());
    }
  }
  let previous = match fs::read_to_string(file) {
    Ok(value) => Some(value),
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
    Err(e) => return Err(e.to_string()),
  };
  let next = render(previous.as_deref().unwrap_or(""), paths)?;
  if previous.as_deref() == Some(&next) {
    return Ok(false);
  }
  let parent = file.parent().ok_or("Shell configuration directory not found")?;
  fs::create_dir_all(parent).map_err(|e| e.to_string())?;
  let suffix = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_nanos();
  let name = file.file_name().ok_or("Shell configuration filename is missing")?.to_string_lossy();
  if let Some(ref old) = previous {
    fs::DirBuilder::new().recursive(true).mode(0o700).create(backups).map_err(|e| e.to_string())?;
    let backup = backups.join(format!("{}.{suffix}", name.trim_start_matches('.')));
    let mut out = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&backup).map_err(|e| e.to_string())?;
    out.write_all(old.as_bytes()).map_err(|e| e.to_string())?;
    out.sync_all().map_err(|e| e.to_string())?;
  }
  let temporary = parent.join(format!(".{name}.tendkit-{suffix}.tmp"));
  let result = (|| {
    let mode = fs::metadata(file).map(|m| m.permissions().mode() & 0o777).unwrap_or(0o600);
    let mut out = OpenOptions::new().write(true).create_new(true).mode(mode).open(&temporary).map_err(|e| e.to_string())?;
    out.write_all(next.as_bytes()).map_err(|e| e.to_string())?;
    out.sync_all().map_err(|e| e.to_string())?;
    if fs::symlink_metadata(file).map(|m| m.file_type().is_symlink()).unwrap_or(false) || fs::read_to_string(file).ok() != previous {
      return Err("The shell configuration file changed during the operation. Try again.".into());
    }
    fs::rename(&temporary, file).map_err(|e| e.to_string())
  })();
  if result.is_err() {
    let _ = fs::remove_file(&temporary);
  }
  result.map(|_| true)
}

#[cfg(test)]
mod tests {
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
}
