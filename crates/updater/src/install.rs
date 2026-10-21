use crate::{
  process::{download, get, run},
  release::checksum_for,
  Release, UpdateError,
};
use std::{
  fs,
  os::unix::{fs::DirBuilderExt, process::CommandExt},
  path::{Path, PathBuf},
  process::{Command, Stdio},
};

const APP_NAME: &str = "TendKit.app";

/// The steps of preparing an update, reported to the interface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
  Downloading,
  Verifying,
}

/// A verified replacement app waiting to be swapped in.
#[derive(Clone, Debug)]
pub struct Staged {
  pub app: PathBuf,
  pub directory: PathBuf,
}

/// The `.app` bundle the running executable lives in.
pub fn current_app_bundle() -> Result<PathBuf, UpdateError> {
  let exe = std::env::current_exe().map_err(|e| UpdateError::new(format!("Could not locate TendKit: {e}")))?;
  bundle_of(&exe).ok_or_else(|| UpdateError::new("Updates are available when TendKit runs from TendKit.app."))
}

pub(crate) fn bundle_of(executable: &Path) -> Option<PathBuf> {
  let bundle = executable.parent()?.parent()?.parent()?;
  (bundle.extension()? == "app" && executable.parent()?.file_name()? == "MacOS").then(|| bundle.to_path_buf())
}

/// Downloads the release next to the running app, verifies it, and returns the
/// replacement. Staging beside the app keeps the final swap an atomic rename.
pub fn prepare(release: &Release, current_app: &Path, mut on_stage: impl FnMut(Stage)) -> Result<Staged, UpdateError> {
  let parent = current_app.parent().ok_or_else(|| UpdateError::new("Could not locate the TendKit folder."))?;
  let directory = parent.join(format!(".TendKit-update-{}", std::process::id()));
  let _ = fs::remove_dir_all(&directory);
  fs::DirBuilder::new()
    .mode(0o700)
    .create(&directory)
    .map_err(|_| UpdateError::new(format!("TendKit cannot write to {}. Move it to Applications or fix the folder permissions.", parent.display())))?;
  let result = stage(release, current_app, &directory, &mut on_stage);
  if result.is_err() {
    let _ = fs::remove_dir_all(&directory);
  }
  result
}

fn stage(release: &Release, current_app: &Path, directory: &Path, on_stage: &mut impl FnMut(Stage)) -> Result<Staged, UpdateError> {
  on_stage(Stage::Downloading);
  let archive = directory.join(&release.archive_name);
  download(&release.archive_url, &archive)?;
  let (status, listing) = get(&release.checksums_url)?;
  if status != 200 {
    return Err(UpdateError::new("Could not download the release checksums."));
  }

  on_stage(Stage::Verifying);
  let expected = checksum_for(&listing, &release.archive_name).ok_or_else(|| UpdateError::new("The release checksums do not list the download."))?;
  let actual = run("/usr/bin/shasum", &["-a", "256", &archive.to_string_lossy()])?;
  if actual.split_whitespace().next().map(str::to_ascii_lowercase).as_deref() != Some(expected.as_str()) {
    return Err(UpdateError::new("The download does not match its published checksum."));
  }
  let extracted = directory.join("extracted");
  run("/usr/bin/ditto", &["-x", "-k", &archive.to_string_lossy(), &extracted.to_string_lossy()])?;
  let app = extracted.join(APP_NAME);
  if !app.is_dir() {
    return Err(UpdateError::new("The download does not contain TendKit.app."));
  }
  verify_app(&app, current_app, &release.version)?;
  let replacement = directory.join(APP_NAME);
  fs::rename(&app, &replacement).map_err(|e| UpdateError::new(format!("Could not stage the update: {e}")))?;
  Ok(Staged { app: replacement, directory: directory.to_path_buf() })
}

fn verify_app(app: &Path, current_app: &Path, version: &str) -> Result<(), UpdateError> {
  let path = app.to_string_lossy();
  run("/usr/bin/codesign", &["--verify", "--deep", "--strict", &path])
    .map_err(|e| UpdateError::new(format!("The update's code signature is invalid. {e}")))?;
  run("/usr/sbin/spctl", &["--assess", "--type", "execute", &path]).map_err(|e| UpdateError::new(format!("macOS rejected the update. {e}")))?;
  let new_team = team_identifier(&run("/usr/bin/codesign", &["-dv", "--verbose=2", &path])?);
  let current_team = team_identifier(&run("/usr/bin/codesign", &["-dv", "--verbose=2", &current_app.to_string_lossy()])?);
  if current_team.is_some() && current_team != new_team {
    return Err(UpdateError::new("The update is signed by a different developer than the installed app."));
  }
  let plist =
    |app: &Path, key: &str| run("/usr/libexec/PlistBuddy", &["-c", &format!("Print :{key}"), &app.join("Contents/Info.plist").to_string_lossy()]);
  if plist(app, "CFBundleIdentifier")?.trim() != plist(current_app, "CFBundleIdentifier")?.trim() {
    return Err(UpdateError::new("The update is for a different application."));
  }
  if plist(app, "CFBundleShortVersionString")?.trim() != version {
    return Err(UpdateError::new("The update's version does not match the release."));
  }
  Ok(())
}

/// The `TeamIdentifier` from `codesign -dv` output; `None` for ad-hoc signatures.
pub(crate) fn team_identifier(output: &str) -> Option<String> {
  output
    .lines()
    .find_map(|line| line.strip_prefix("TeamIdentifier="))
    .map(str::trim)
    .filter(|team| *team != "not set" && !team.is_empty())
    .map(str::to_string)
}

/// Removes a staged update that will not be installed.
pub fn discard(staged: &Staged) {
  let _ = fs::remove_dir_all(&staged.directory);
}

/// Waits for this process to exit, swaps the staged app into place, and reopens
/// TendKit. The caller must quit right after this returns. A failed swap restores
/// the previous app and reopens it.
pub fn install(staged: &Staged, current_app: &Path) -> Result<(), UpdateError> {
  let script = staged.directory.join("swap.sh");
  fs::write(&script, SWAP_SCRIPT).map_err(|e| UpdateError::new(format!("Could not prepare the installer: {e}")))?;
  Command::new("/bin/bash")
    .arg(&script)
    .arg(std::process::id().to_string())
    .arg(current_app)
    .arg(&staged.app)
    .arg(&staged.directory)
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    // Its own process group, so it outlives this app.
    .process_group(0)
    .spawn()
    .map(|_| ())
    .map_err(|e| UpdateError::new(format!("Could not start the installer: {e}")))
}

const SWAP_SCRIPT: &str = r#"#!/bin/bash
pid="$1"; current="$2"; replacement="$3"; staging="$4"
for _ in $(seq 1 150); do kill -0 "$pid" 2>/dev/null || break; sleep 0.2; done
backup="$staging/previous.app"
if mv "$current" "$backup" && mv "$replacement" "$current"; then
  rm -rf "$staging"
elif [ -d "$backup" ] && [ ! -d "$current" ]; then
  mv "$backup" "$current"
fi
open "$current"
"#;
