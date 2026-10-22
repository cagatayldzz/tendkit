use std::{
  fs,
  os::unix::fs::{OpenOptionsExt, PermissionsExt},
  path::Path,
  time::Duration,
};
use tendkit_plan::valid_version;
use tendkit_process::{command, curl, Control, Sink};
use tendkit_protocol::{InstallStep, Recipe};

pub(super) trait Runner {
  fn check(&self) -> Result<(), String> {
    Ok(())
  }

  fn download(&mut self, url: &str, path: &Path) -> Result<(), String>;

  fn metadata(&mut self, url: &str) -> Result<serde_json::Value, String>;

  fn run(&mut self, path: &Path, args: &[String]) -> Result<String, String>;
}

pub(super) struct SystemRunner<'a, 'b> {
  pub control: &'a Control,
  pub sink: &'a mut Sink<'b>,
}

impl Runner for SystemRunner<'_, '_> {
  fn check(&self) -> Result<(), String> {
    self.control.check()
  }

  fn download(&mut self, url: &str, path: &Path) -> Result<(), String> {
    curl(url, Some(path), self.control, self.sink).map(|_| ())
  }

  fn metadata(&mut self, url: &str) -> Result<serde_json::Value, String> {
    serde_json::from_str(&curl(url, None, self.control, self.sink)?).map_err(|e| e.to_string())
  }

  fn run(&mut self, path: &Path, args: &[String]) -> Result<String, String> {
    command(path, args, self.control, self.sink, true, Duration::from_secs(1800), &[])
  }
}

fn checksum(value: &str) -> Result<&str, String> {
  if value.len() == 64 && value.bytes().all(|c| c.is_ascii_hexdigit()) {
    Ok(value)
  } else {
    Err("The publisher did not provide a valid SHA-256 checksum".into())
  }
}

/// Only accept the exact asset on the official repository and pinned release.
fn github_asset(data: &serde_json::Value, repository: &str, tag: &str, name: &str) -> Result<(String, String), String> {
  if data["tag_name"] != tag {
    return Err("The release tag does not match the approved version".into());
  }
  let asset = data["assets"].as_array().and_then(|items| items.iter().find(|a| a["name"] == name)).ok_or("Release asset not found")?;
  let url = format!("https://github.com/{repository}/releases/download/{tag}/{name}");
  if asset["browser_download_url"] != url {
    return Err("Unexpected release asset URL".into());
  }
  let digest = asset["digest"].as_str().and_then(|s| s.strip_prefix("sha256:")).ok_or("Release checksum is missing")?;
  Ok((url, checksum(digest)?.into()))
}

pub(super) fn install(step: &InstallStep, work: &Path, destination: &Path, runner: &mut impl Runner) -> Result<(), String> {
  if !valid_version(&step.target) {
    return Err("Invalid artifact version".into());
  }
  let arch = if cfg!(target_arch = "aarch64") { "aarch64" } else { "x86_64" };
  let (url, digest, member) = match step.recipe {
    Recipe::Bun | Recipe::Codex => {
      let (repo, tag, name, member) = if matches!(step.recipe, Recipe::Bun) {
        let platform = if arch == "aarch64" { "arm64" } else { "x64" };
        let stem = format!("bun-darwin-{platform}");
        ("oven-sh/bun", format!("bun-v{}", step.target), format!("{stem}.zip"), format!("{stem}/bun"))
      } else {
        let stem = format!("codex-{arch}-apple-darwin");
        ("openai/codex", format!("rust-v{}", step.target), format!("{stem}.tar.gz"), stem)
      };
      let data = runner.metadata(&format!("https://api.github.com/repos/{repo}/releases/tags/{tag}"))?;
      let (url, digest) = github_asset(&data, repo, &tag, &name)?;
      (url, digest, Some(member))
    }
    Recipe::Claude { .. } => {
      let platform = if arch == "aarch64" { "darwin-arm64" } else { "darwin-x64" };
      let base = format!("https://downloads.claude.ai/claude-code-releases/{}", step.target);
      let data = runner.metadata(&format!("{base}/manifest.json"))?;
      let digest = checksum(data["platforms"][platform]["checksum"].as_str().ok_or("Claude checksum is missing")?)?.into();
      (format!("{base}/{platform}/claude"), digest, None)
    }
    _ => return Err("Unsupported artifact recipe".into()),
  };
  let archive = work.join("download");
  runner.download(&url, &archive)?;
  let actual = runner.run(Path::new("/usr/bin/shasum"), &["-a".into(), "256".into(), archive.to_string_lossy().into_owned()])?;
  if !actual.split_whitespace().next().is_some_and(|hash| hash.eq_ignore_ascii_case(&digest)) {
    return Err("The downloaded artifact checksum does not match".into());
  }
  if let Some(member) = member {
    // Extract only the expected binary, never arbitrary archive paths or scripts.
    let binary = if matches!(step.recipe, Recipe::Bun) {
      runner.run(
        Path::new("/usr/bin/unzip"),
        &["-o".into(), archive.to_string_lossy().into_owned(), member.clone(), "-d".into(), work.to_string_lossy().into_owned()],
      )?;
      work.join(member)
    } else {
      runner.run(
        Path::new("/usr/bin/tar"),
        &["-xzf".into(), archive.to_string_lossy().into_owned(), "-C".into(), work.to_string_lossy().into_owned(), member.clone()],
      )?;
      work.join(member)
    };
    if !fs::symlink_metadata(&binary).map_err(|e| e.to_string())?.file_type().is_file() {
      return Err("The archive binary is not a regular file".into());
    }
    runner.check()?;
    let parent = destination.parent().ok_or("Invalid binary destination")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    // Stage beside the destination so a failed copy cannot truncate a working tool.
    let staged = parent.join(format!(".{}-{}", work.file_name().ok_or("Invalid work directory")?.to_string_lossy(), step.id));
    let result = (|| {
      let mut output = fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&staged).map_err(|e| e.to_string())?;
      let mut input = fs::File::open(&binary).map_err(|e| e.to_string())?;
      std::io::copy(&mut input, &mut output).map_err(|e| e.to_string())?;
      output.sync_all().map_err(|e| e.to_string())?;
      fs::set_permissions(&staged, fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
      runner.check()?;
      fs::rename(&staged, destination).map_err(|e| e.to_string())
    })();
    let _ = fs::remove_file(staged);
    result
  } else {
    runner.check()?;
    fs::set_permissions(&archive, fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    runner.run(&archive, &["install".into(), step.target.clone()]).map(|_| ())
  }
}

#[cfg(test)]
#[path = "../tests/artifact.rs"]
mod tests;
