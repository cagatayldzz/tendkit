use std::{
  fs::{self, OpenOptions},
  io::{self, Write},
  path::PathBuf,
  sync::atomic::{AtomicU64, Ordering},
  time::{SystemTime, UNIX_EPOCH},
};

/// Owns one operation's extracted engine until the child has exited.
pub(crate) struct EmbeddedEngine {
  directory: PathBuf,
  pub(crate) executable: PathBuf,
}

impl EmbeddedEngine {
  pub(crate) fn extract(bytes: &[u8]) -> io::Result<Self> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let directory =
      std::env::temp_dir().join(format!("tendkit-engine-{}-{timestamp}-{}", std::process::id(), COUNTER.fetch_add(1, Ordering::Relaxed)));
    // Never reuse a directory or executable from an earlier operation.
    fs::create_dir(&directory)?;
    let engine = Self { executable: directory.join("tendkit-engine.exe"), directory };
    let mut file = OpenOptions::new().write(true).create_new(true).open(&engine.executable)?;
    file.write_all(bytes)?;
    file.flush()?;
    Ok(engine)
  }
}

impl Drop for EmbeddedEngine {
  fn drop(&mut self) {
    let _ = fs::remove_file(&self.executable);
    let _ = fs::remove_dir(&self.directory);
  }
}

#[cfg(test)]
#[path = "../tests/embedded.rs"]
mod tests;
