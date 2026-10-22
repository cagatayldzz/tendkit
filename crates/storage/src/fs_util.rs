use std::{fs, io, io::Write, path::Path};

/// Writes to a sibling temporary file and renames it over `path`, so readers
/// never see a partial file. `mode` applies on Unix.
pub fn write_atomic(path: &Path, data: &[u8], mode: u32) -> io::Result<()> {
  let name = path.file_name().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Missing file name"))?;
  let temporary = path.with_file_name(format!(".{}.tendkit-{}", name.to_string_lossy(), std::process::id()));
  let result = (|| {
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
      use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
      options.mode(mode);
      let mut file = options.open(&temporary)?;
      file.set_permissions(fs::Permissions::from_mode(mode))?;
      file.write_all(data)?;
      file.sync_all()?;
    }

    #[cfg(not(unix))]
    {
      let _ = mode;
      let mut file = options.open(&temporary)?;
      file.write_all(data)?;
      file.sync_all()?;
    }
    fs::rename(&temporary, path)
  })();
  if result.is_err() {
    let _ = fs::remove_file(&temporary);
  }
  result
}

#[cfg(unix)]
pub fn mode(metadata: &fs::Metadata) -> u32 {
  use std::os::unix::fs::PermissionsExt;
  metadata.permissions().mode() & 0o7777
}

#[cfg(not(unix))]
pub fn mode(_: &fs::Metadata) -> u32 {
  0o644
}
