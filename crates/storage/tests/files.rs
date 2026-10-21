use std::{
  fs,
  os::unix::fs::PermissionsExt,
  path::PathBuf,
  time::{Duration, UNIX_EPOCH},
};
use tendkit_storage::{latest_cache::LatestVersionCache, shell_context, OpenCommitConfig};

fn temp_path(name: &str) -> PathBuf {
  static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
  let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  std::env::temp_dir().join(format!("tendkit-{name}-{}-{n}", std::process::id()))
}

fn permissions(path: &PathBuf) -> u32 {
  fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[test]
fn latest_version_cache_persists_across_instances_and_expires() {
  let path = temp_path("latest").with_extension("json");
  let key = LatestVersionCache::key("bun", Some("Bun"));
  let checked_at = UNIX_EPOCH + Duration::from_secs(1_000_000);
  let mut cache = LatestVersionCache::load(path.clone());
  cache.store("1.4.3", &key, checked_at);

  let reloaded = LatestVersionCache::load(path.clone());
  assert_eq!(reloaded.fresh(&key, checked_at + Duration::from_secs(60)).map(|e| e.version.as_str()), Some("1.4.3"));
  assert!(reloaded.fresh(&key, checked_at + LatestVersionCache::LIFETIME).is_none());
  assert_eq!(reloaded.entry(&key).map(|e| e.version.as_str()), Some("1.4.3"));
  assert!(reloaded.fresh(&LatestVersionCache::key("bun", Some("Homebrew")), checked_at).is_none());

  cache.remove_all();
  assert!(LatestVersionCache::load(path).entry(&key).is_none());
}

#[test]
fn latest_version_cache_ignores_unreadable_files() {
  let path = temp_path("latest-invalid").with_extension("json");
  fs::write(&path, "not json").unwrap();
  assert!(LatestVersionCache::load(path.clone()).entry("node|Homebrew").is_none());
  let _ = fs::remove_file(path);
}

#[test]
fn opencommit_config_edits_values_in_place_and_preserves_other_lines() {
  let mut config = OpenCommitConfig::parse("OCO_API_KEY=secret\nOCO_MODEL=gpt-4o-mini\nOCO_EMOJI=false\n");
  config.set("OCO_MODEL", Some("gpt-5-mini"));
  config.set_bool("OCO_EMOJI", true);
  config.set("OCO_LANGUAGE", Some("tr"));
  assert_eq!(config.contents(), "OCO_API_KEY=secret\nOCO_MODEL=gpt-5-mini\nOCO_EMOJI=true\nOCO_LANGUAGE=tr\n");
}

#[test]
fn opencommit_config_reads_undefined_and_quoted_values() {
  let config = OpenCommitConfig::parse("OCO_API_URL=undefined\nOCO_MODEL=\"a;b\"\nOCO_WHY=true\n");
  assert_eq!(config.get("OCO_API_URL"), None);
  assert_eq!(config.get("OCO_MODEL").as_deref(), Some("a;b"));
  assert!(config.bool("OCO_WHY", false));
  assert!(config.bool("OCO_GITPUSH", true));
}

#[test]
fn opencommit_config_quotes_values_that_ini_would_misread() {
  let mut config = OpenCommitConfig::default();
  config.set("OCO_MODEL", Some("model#1"));
  assert_eq!(config.contents(), "OCO_MODEL=\"model#1\"\n");
  assert_eq!(config.get("OCO_MODEL").as_deref(), Some("model#1"));
}

#[test]
fn opencommit_config_saves_privately_and_round_trips() {
  let path = temp_path("opencommit");
  let mut config = OpenCommitConfig::default();
  config.set("OCO_LANGUAGE", Some("en"));
  config.save(&path).unwrap();
  assert_eq!(OpenCommitConfig::load(&path).unwrap(), config);
  assert_eq!(permissions(&path), 0o600);
  let _ = fs::remove_file(path);
}

#[test]
fn shell_context_appends_replaces_and_removes_the_block_keeping_user_content() {
  let original = "export EDITOR=vim\n# >>> TendKit >>>\nexport PATH=\"$PATH\":/opt/homebrew/bin\n# <<< TendKit <<<\n";
  let added = shell_context::render(original, "Keep it short").unwrap();
  assert!(added.starts_with(original));
  assert_eq!(shell_context::instruction(&added).unwrap().as_deref(), Some("Keep it short"));
  assert!(added.contains("oco() { command oco --context \"$TENDKIT_OCO_CONTEXT\" \"$@\"; }"));

  let replaced = shell_context::render(&added, "It's shorter").unwrap();
  assert_eq!(shell_context::instruction(&replaced).unwrap().as_deref(), Some("It's shorter"));
  assert!(replaced.contains("TENDKIT_OCO_CONTEXT='It'\\''s shorter'"));
  assert_eq!(shell_context::render(&replaced, "It's shorter").unwrap(), replaced);

  assert_eq!(shell_context::render(&replaced, "  ").unwrap(), original);
}

#[test]
fn shell_context_rejects_duplicate_or_incomplete_blocks() {
  let block = shell_context::render("", "A").unwrap();
  assert!(shell_context::render(&format!("{block}{block}"), "B").is_err());
  assert!(shell_context::instruction("# >>> TendKit OpenCommit >>>\n").is_err());
}

#[test]
fn shell_context_saves_with_backup_and_keeps_permissions() {
  let directory = temp_path("zshrc");
  fs::create_dir_all(&directory).unwrap();
  let file = directory.join(".zshrc");
  let backups = directory.join("backups");
  fs::write(&file, "alias ll='ls -l'\n").unwrap();
  fs::set_permissions(&file, fs::Permissions::from_mode(0o640)).unwrap();

  shell_context::save("Short", &file, &backups).unwrap();
  assert_eq!(shell_context::load(&file).unwrap().as_deref(), Some("Short"));
  assert_eq!(fs::read_dir(&backups).unwrap().count(), 1);
  assert_eq!(permissions(&file), 0o640);

  let link = directory.join("linked");
  std::os::unix::fs::symlink(&file, &link).unwrap();
  assert!(matches!(shell_context::save("Other", &link, &backups), Err(shell_context::Failure::NotRegularFile(_))));
  let _ = fs::remove_dir_all(directory);
}
