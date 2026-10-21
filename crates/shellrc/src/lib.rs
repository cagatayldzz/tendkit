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
mod tests;
