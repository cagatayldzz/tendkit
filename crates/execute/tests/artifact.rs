use super::*;
use serde_json::json;
use tendkit_protocol::StepAction;

#[derive(Default)]
struct FakeRunner {
  calls: Vec<String>,
  hash: String,
  fail_download: bool,
}

impl Runner for FakeRunner {
  fn metadata(&mut self, url: &str) -> Result<serde_json::Value, String> {
    self.calls.push(url.into());
    Ok(json!({"platforms": {"darwin-arm64": {"checksum": "a".repeat(64)}, "darwin-x64": {"checksum": "a".repeat(64)}}}))
  }

  fn download(&mut self, _: &str, path: &Path) -> Result<(), String> {
    self.calls.push("download".into());
    if self.fail_download {
      return Err("Operation cancelled".into());
    }
    fs::write(path, b"fake binary").map_err(|e| e.to_string())
  }

  fn run(&mut self, path: &Path, _: &[String]) -> Result<String, String> {
    self.calls.push(path.to_string_lossy().into_owned());
    Ok(self.hash.clone())
  }
}

fn step() -> InstallStep {
  InstallStep {
    id: "claude".into(),
    title: "Claude Code".into(),
    target: "2.1.0".into(),
    recipe: Recipe::Claude { channel: "latest".into() },
    action: StepAction::Install,
    before: None,
    dependencies: vec![],
  }
}

#[test]
fn checksum_failure_never_executes_download() {
  let work = crate::WorkDir::new().unwrap();
  let mut runner = FakeRunner { hash: "b".repeat(64), ..Default::default() };
  assert!(install(&step(), &work.0, &work.0.join("unused"), &mut runner).unwrap_err().contains("checksum does not match"));
  assert_eq!(runner.calls.len(), 3);
  assert_eq!(runner.calls.last().unwrap(), "/usr/bin/shasum");
}

#[test]
fn cancellation_stops_before_verification_or_execution() {
  let work = crate::WorkDir::new().unwrap();
  let mut runner = FakeRunner { fail_download: true, ..Default::default() };
  assert!(install(&step(), &work.0, &work.0.join("unused"), &mut runner).unwrap_err().contains("cancelled"));
  assert_eq!(runner.calls.len(), 2);
}

#[test]
fn verified_binary_runs_only_after_checksum() {
  let work = crate::WorkDir::new().unwrap();
  let mut runner = FakeRunner { hash: "a".repeat(64), ..Default::default() };
  install(&step(), &work.0, &work.0.join("unused"), &mut runner).unwrap();
  assert_eq!(runner.calls[2], "/usr/bin/shasum");
  assert_eq!(runner.calls[3], work.0.join("download").to_string_lossy());
}

#[test]
fn release_metadata_rejects_wrong_origin_tag_or_digest() {
  let mut data = json!({"tag_name": "bun-v1.2.0", "assets": [{"name": "bun.zip", "browser_download_url": "https://github.com/oven-sh/bun/releases/download/bun-v1.2.0/bun.zip", "digest": format!("sha256:{}", "a".repeat(64))}]});
  assert!(github_asset(&data, "oven-sh/bun", "bun-v1.2.0", "bun.zip").is_ok());
  assert!(github_asset(&data, "oven-sh/bun", "bun-v1.3.0", "bun.zip").is_err());
  data["assets"][0]["browser_download_url"] = json!("https://example.com/bun.zip");
  assert!(github_asset(&data, "oven-sh/bun", "bun-v1.2.0", "bun.zip").is_err());
  data["assets"][0]["browser_download_url"] = json!("https://github.com/oven-sh/bun/releases/download/bun-v1.2.0/bun.zip");
  data["assets"][0]["digest"] = json!("sha256:bad");
  assert!(github_asset(&data, "oven-sh/bun", "bun-v1.2.0", "bun.zip").is_err());
}

#[test]
fn homebrew_requires_expected_leaf_signer_and_trust() {
  assert!(crate::valid_homebrew_signature(
    "Status: signed by a certificate trusted by macOS\n  1. Developer ID Installer: Patrick Linnane (927JGANW46)\n"
  ));
  assert!(!crate::valid_homebrew_signature("Status: signed by a certificate trusted by macOS\n  1. Developer ID Installer: Someone Else (UNKNOWN)\n  2. Developer ID Installer: Patrick Linnane (927JGANW46)"));
  assert!(!crate::valid_homebrew_signature("1. Developer ID Installer: Patrick Linnane (927JGANW46)"));
}

struct ArchiveRunner {
  fail_extract: bool,
}

impl Runner for ArchiveRunner {
  fn metadata(&mut self, url: &str) -> Result<serde_json::Value, String> {
    let tag = url.rsplit('/').next().unwrap();
    let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "x64" };
    let name = format!("bun-darwin-{arch}.zip");
    Ok(json!({"tag_name": tag, "assets": [{"name": name,
      "browser_download_url": format!("https://github.com/oven-sh/bun/releases/download/{tag}/{name}"),
      "digest": format!("sha256:{}", "a".repeat(64))}]}))
  }

  fn download(&mut self, _: &str, _: &Path) -> Result<(), String> {
    Ok(())
  }

  fn run(&mut self, path: &Path, args: &[String]) -> Result<String, String> {
    if path == Path::new("/usr/bin/shasum") {
      return Ok("a".repeat(64));
    }
    if self.fail_extract {
      return Err("Fake extraction failure".into());
    }
    assert_eq!(path, Path::new("/usr/bin/unzip"));
    let binary = Path::new(&args[4]).join(&args[2]);
    fs::create_dir_all(binary.parent().unwrap()).unwrap();
    fs::write(binary, b"verified replacement").unwrap();
    Ok(String::new())
  }
}

#[test]
fn archive_failure_preserves_existing_binary_and_success_replaces_it() {
  let work = crate::WorkDir::new().unwrap();
  let destination = work.0.join("bin/bun");
  fs::create_dir_all(destination.parent().unwrap()).unwrap();
  fs::write(&destination, b"old binary").unwrap();
  let mut step = step();
  step.id = "bun".into();
  step.recipe = Recipe::Bun;
  assert!(install(&step, &work.0, &destination, &mut ArchiveRunner { fail_extract: true }).is_err());
  assert_eq!(fs::read(&destination).unwrap(), b"old binary");
  install(&step, &work.0, &destination, &mut ArchiveRunner { fail_extract: false }).unwrap();
  assert_eq!(fs::read(&destination).unwrap(), b"verified replacement");
  assert_eq!(fs::metadata(&destination).unwrap().permissions().mode() & 0o777, 0o755);
}
