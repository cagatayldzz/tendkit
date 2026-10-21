//! OpenCommit has no config key for extra prompt instructions; they can only be
//! passed with `--context`. This manages a `~/.zshrc` block that wraps `oco` and
//! `opencommit` in shell functions adding that flag. It follows the engine's
//! PATH block rules: regular files only, a backup before every change, and an
//! atomic write that keeps the file's permissions.

use crate::{
  fs_util::{mode, write_atomic},
  paths::{home_dir, support_dir},
};
use std::{
  fmt, fs, io,
  ops::Range,
  path::{Path, PathBuf},
  time::{SystemTime, UNIX_EPOCH},
};

pub const DEFAULT_INSTRUCTION: &str =
  "Write a short subject of at most 60 characters that names only the main change. No filler such as for better user experience.";

const BEGIN: &str = "# >>> TendKit OpenCommit >>>";
const END: &str = "# <<< TendKit OpenCommit <<<";
const VARIABLE: &str = "TENDKIT_OCO_CONTEXT";
const MAX_SIZE: u64 = 1_048_576;

pub fn default_path() -> PathBuf {
  home_dir().join(".zshrc")
}
pub fn backups_path() -> PathBuf {
  support_dir().join("backups")
}

#[derive(Debug)]
pub enum Failure {
  NotRegularFile(String),
  TooLarge,
  MalformedBlock,
  ChangedDuringWrite,
  Io(io::Error),
}

impl fmt::Display for Failure {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::NotRegularFile(path) => write!(f, "{path} is not a regular file. Commit instructions were not saved."),
      Self::TooLarge => f.write_str("The shell configuration file is too large."),
      Self::MalformedBlock => f.write_str("The TendKit OpenCommit block in .zshrc is incomplete or duplicated. Check the file."),
      Self::ChangedDuringWrite => f.write_str("The shell configuration file changed while saving. Try again."),
      Self::Io(error) => write!(f, "{error}"),
    }
  }
}

impl std::error::Error for Failure {}

impl From<io::Error> for Failure {
  fn from(error: io::Error) -> Self {
    Self::Io(error)
  }
}

/// The instruction in the existing block, or None when there is no block.
pub fn instruction(contents: &str) -> Result<Option<String>, Failure> {
  let Some(range) = block_range(contents)? else { return Ok(None) };
  let prefix = format!("{VARIABLE}=");
  Ok(contents[range].split('\n').find_map(|line| line.strip_prefix(&prefix)).map(unquote))
}

/// Replaces or appends the block; an empty instruction removes it.
pub fn render(contents: &str, instruction: &str) -> Result<String, Failure> {
  let trimmed = instruction.trim();
  let mut result = contents.to_string();
  if let Some(range) = block_range(contents)? {
    if trimmed.is_empty() {
      // Drop the block together with its trailing newline.
      let end = if result[range.end..].starts_with('\n') { range.end + 1 } else { range.end };
      result.replace_range(range.start..end, "");
    } else {
      result.replace_range(range, &block(trimmed));
    }
  } else if !trimmed.is_empty() {
    if !result.is_empty() && !result.ends_with('\n') {
      result.push('\n');
    }
    result.push_str(&block(trimmed));
    result.push('\n');
  }
  Ok(result)
}

pub fn load(path: &Path) -> Result<Option<String>, Failure> {
  match read(path)? {
    Some(contents) => instruction(&contents),
    None => Ok(None),
  }
}

pub fn save(instruction: &str, path: &Path, backups: &Path) -> Result<(), Failure> {
  let previous = read(path)?;
  let next = render(previous.as_deref().unwrap_or(""), instruction)?;
  if next == previous.as_deref().unwrap_or("") {
    return Ok(());
  }
  if let Some(previous) = &previous {
    create_private_dir(backups)?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_micros()).unwrap_or(0);
    let name = path.file_name().map(|n| n.to_string_lossy().trim_matches('.').to_string()).unwrap_or_default();
    write_atomic(&backups.join(format!("{name}.{stamp}")), previous.as_bytes(), 0o600)?;
  }
  let file_mode = fs::metadata(path).map(|m| mode(&m)).unwrap_or(0o644);
  // Re-check right before replacing so a concurrent edit is never overwritten.
  if read(path)? != previous {
    return Err(Failure::ChangedDuringWrite);
  }
  write_atomic(path, next.as_bytes(), file_mode)?;
  Ok(())
}

fn create_private_dir(path: &Path) -> io::Result<()> {
  let mut builder = fs::DirBuilder::new();
  builder.recursive(true);
  #[cfg(unix)]
  {
    use std::os::unix::fs::DirBuilderExt;
    builder.mode(0o700);
  }
  builder.create(path)
}

fn read(path: &Path) -> Result<Option<String>, Failure> {
  let Ok(metadata) = fs::symlink_metadata(path) else { return Ok(None) };
  if !metadata.file_type().is_file() {
    return Err(Failure::NotRegularFile(path.display().to_string()));
  }
  if metadata.len() > MAX_SIZE {
    return Err(Failure::TooLarge);
  }
  Ok(Some(fs::read_to_string(path)?))
}

fn block_range(contents: &str) -> Result<Option<Range<usize>>, Failure> {
  let starts: Vec<usize> = contents.match_indices(BEGIN).map(|(i, _)| i).collect();
  let ends: Vec<usize> = contents.match_indices(END).map(|(i, _)| i).collect();
  if starts.is_empty() && ends.is_empty() {
    return Ok(None);
  }
  match (starts.as_slice(), ends.as_slice()) {
    ([start], [end]) if start < end && (*start == 0 || contents.as_bytes()[start - 1] == b'\n') => Ok(Some(*start..end + END.len())),
    _ => Err(Failure::MalformedBlock),
  }
}

fn block(instruction: &str) -> String {
  format!(
    "{BEGIN}\n{VARIABLE}={}\noco() {{ command oco --context \"${VARIABLE}\" \"$@\"; }}\nopencommit() {{ command opencommit --context \"${VARIABLE}\" \"$@\"; }}\n{END}",
    quote(instruction)
  )
}

pub(crate) fn quote(value: &str) -> String {
  format!("'{}'", value.replace('\'', "'\\''"))
}

pub(crate) fn unquote(value: &str) -> String {
  match value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')) {
    Some(inner) if value.len() >= 2 => inner.replace("'\\''", "'"),
    _ => value.to_string(),
  }
}
