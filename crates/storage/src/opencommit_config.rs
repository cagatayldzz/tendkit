use crate::{fs_util::write_atomic, paths::home_dir};
use std::{
  fs, io,
  path::{Path, PathBuf},
};

/// OpenCommit's global INI-style `~/.opencommit` file. Edits replace values in
/// place and keep every other line, including API keys, untouched.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OpenCommitConfig {
  lines: Vec<String>,
}

impl OpenCommitConfig {
  /// Language codes OpenCommit ships translations for.
  pub const LANGUAGES: &'static [&'static str] =
    &["en", "cs", "de", "es_ES", "fr", "id_ID", "it", "ja", "ko", "nl", "pl", "pt_br", "ru", "sv", "th", "tr", "vi_VN", "zh_CN", "zh_TW"];
  /// Models offered in TendKit's settings.
  pub const MODELS: &'static [&'static str] = &["gpt-4o-mini"];
  /// Diffs larger than OCO_TOKENS_MAX_INPUT are split per file and their
  /// messages joined with "; ", so TendKit raises OpenCommit's 4,096 default.
  pub const INPUT_TOKEN_LIMITS: &'static [u32] = &[4_096, 16_000, 32_000, 64_000, 128_000];
  pub const OPENCOMMIT_DEFAULT_INPUT_TOKENS: u32 = 4_096;
  pub const RECOMMENDED_INPUT_TOKENS: u32 = 32_000;
  /// TendKit always generates Conventional Commits.
  pub const PROMPT_MODULE: &'static str = "conventional-commit";

  pub fn default_path() -> PathBuf {
    home_dir().join(".opencommit")
  }

  pub fn parse(contents: &str) -> Self {
    let mut lines: Vec<String> = if contents.is_empty() { Vec::new() } else { contents.split('\n').map(String::from).collect() };
    if lines.last().is_some_and(|line| line.is_empty()) {
      lines.pop();
    }
    Self { lines }
  }

  pub fn load(path: &Path) -> io::Result<Self> {
    match fs::read_to_string(path) {
      Ok(contents) => Ok(Self::parse(&contents)),
      Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
      Err(error) => Err(error),
    }
  }

  pub fn contents(&self) -> String {
    if self.lines.is_empty() {
      String::new()
    } else {
      self.lines.join("\n") + "\n"
    }
  }

  pub fn get(&self, key: &str) -> Option<String> {
    let line = &self.lines[self.index(key)?];
    let value = line.split_once('=').map(|(_, value)| value).unwrap_or("").trim();
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
      if let Ok(decoded) = serde_json::from_str::<String>(value) {
        return Some(decoded);
      }
    }
    // OpenCommit stores unset optional values as the literal "undefined".
    (value != "undefined").then(|| value.to_string())
  }

  pub fn set(&mut self, key: &str, value: Option<&str>) {
    let line = format!("{key}={}", value.map(encode).unwrap_or_else(|| "undefined".into()));
    match self.index(key) {
      Some(index) => self.lines[index] = line,
      None => self.lines.push(line),
    }
  }

  pub fn bool(&self, key: &str, fallback: bool) -> bool {
    self.get(key).map(|v| v == "true").unwrap_or(fallback)
  }

  pub fn set_bool(&mut self, key: &str, value: bool) {
    self.set(key, Some(if value { "true" } else { "false" }));
  }

  /// Writes atomically and keeps the file private because it can hold API keys.
  pub fn save(&self, path: &Path) -> io::Result<()> {
    write_atomic(path, self.contents().as_bytes(), 0o600)
  }

  fn index(&self, key: &str) -> Option<usize> {
    self.lines.iter().position(|line| line.split('=').next().map(str::trim) == Some(key))
  }
}

/// INI treats `;` and `#` as comments, so quote values that would be misread.
fn encode(value: &str) -> String {
  let unsafe_char = value.chars().any(|c| "=;#\"\\\n\r".contains(c));
  let padded = value != value.trim_matches(|c| c == ' ' || c == '\t');
  if unsafe_char || padded {
    serde_json::to_string(value).unwrap_or_else(|_| value.into())
  } else {
    value.into()
  }
}
