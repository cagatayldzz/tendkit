use super::*;
use crate::{install::team_identifier, release::checksum_for};
use std::path::Path;

const ARCH: &str = if cfg!(target_arch = "aarch64") { "arm64" } else { "x64" };

const SLUG: &str = "owner/tendkit";

fn release_json(tag: &str, archive_url: &str) -> String {
  format!(
    r#"{{"tag_name":"{tag}","body":"Notes","html_url":"https://github.com/{SLUG}/releases/tag/{tag}","assets":[
      {{"name":"TendKit-{tag}-macos-{ARCH}.zip","browser_download_url":"{archive_url}"}},
      {{"name":"SHA256SUMS.txt","browser_download_url":"https://github.com/{SLUG}/releases/download/{tag}/SHA256SUMS.txt"}}]}}"#
  )
}

#[test]
fn parses_a_release_with_its_assets() {
  let url = format!("https://github.com/{SLUG}/releases/download/v1.2.3/TendKit-v1.2.3-macos-{ARCH}.zip");
  let release = parse_release(&release_json("v1.2.3", &url), SLUG).unwrap();
  assert_eq!(release.version, "1.2.3");
  assert_eq!(release.archive_name, format!("TendKit-v1.2.3-macos-{ARCH}.zip"));
  assert_eq!(release.archive_url, url);
  assert!(release.checksums_url.ends_with("/SHA256SUMS.txt"));
  assert_eq!(release.notes, "Notes");
}

#[test]
fn rejects_assets_hosted_elsewhere_or_for_another_tag() {
  let other_host = "https://evil.example/TendKit-v1.2.3-macos-{ARCH}.zip";
  assert!(parse_release(&release_json("v1.2.3", other_host), SLUG).is_err());
  let other_tag = format!("https://github.com/{SLUG}/releases/download/v9.9.9/TendKit-v1.2.3-macos-{ARCH}.zip");
  assert!(parse_release(&release_json("v1.2.3", &other_tag), SLUG).is_err());
}

#[test]
fn rejects_malformed_tags_and_missing_assets() {
  let url = format!("https://github.com/{SLUG}/releases/download/nightly/TendKit-nightly-macos-{ARCH}.zip");
  assert!(parse_release(&release_json("nightly", &url), SLUG).is_err());
  assert!(parse_release(r#"{"tag_name":"v1.0.0","assets":[]}"#, SLUG).is_err());
  assert!(parse_release("not json", SLUG).is_err());
}

#[test]
fn finds_the_checksum_for_an_archive() {
  let hash = "a".repeat(64);
  let listing = format!("{hash}  TendKit-v1.0.0-macos-{ARCH}.zip\n{}  other.zip\n", "b".repeat(64));
  assert_eq!(checksum_for(&listing, &format!("TendKit-v1.0.0-macos-{ARCH}.zip")), Some(hash));
  assert_eq!(checksum_for(&listing, "missing.zip"), None);
  assert_eq!(checksum_for("short  TendKit.zip", "TendKit.zip"), None);
}

#[test]
fn reads_the_team_identifier_and_ignores_ad_hoc_signatures() {
  assert_eq!(team_identifier("Identifier=x\nTeamIdentifier=ABCDE12345\n"), Some("ABCDE12345".into()));
  assert_eq!(team_identifier("TeamIdentifier=not set\n"), None);
  assert_eq!(team_identifier("Identifier=x\n"), None);
}

#[test]
fn locates_the_app_bundle_from_the_executable_path() {
  assert_eq!(install::bundle_of(Path::new("/Applications/TendKit.app/Contents/MacOS/TendKit")), Some("/Applications/TendKit.app".into()));
  assert_eq!(install::bundle_of(Path::new("/repo/target/debug/TendKit")), None);
}

#[test]
fn the_repository_slug_comes_from_the_workspace() {
  assert_eq!(repository_slug(), Some("cagatayldzz/tendkit"));
}

#[test]
fn release_notes_lose_markdown_markers() {
  let url = format!("https://github.com/{SLUG}/releases/download/v1.0.0/TendKit-v1.0.0-macos-{ARCH}.zip");
  let mut release = parse_release(&release_json("v1.0.0", &url), SLUG).unwrap();
  release.notes = "## New Features\n\n- Added updates ([b096ff5](https://github.com/o/r/commit/b096ff5))\n- See [docs](https://x.y) now".into();
  assert_eq!(release.plain_notes(), "New Features\n\n• Added updates (b096ff5)\n• See docs now");
}
