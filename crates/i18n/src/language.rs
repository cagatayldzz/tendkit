use std::process::Command;

/// A language the interface is translated into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
  English,
  Turkish,
}

impl Language {
  pub const ALL: [Self; 2] = [Self::English, Self::Turkish];

  /// The two-letter code stored in settings and used for `locales/<code>.txt`.
  pub fn code(self) -> &'static str {
    match self {
      Self::English => "en",
      Self::Turkish => "tr",
    }
  }

  /// The language's own name, shown in the language picker.
  pub fn native_name(self) -> &'static str {
    match self {
      Self::English => "English",
      Self::Turkish => "Türkçe",
    }
  }

  pub fn from_code(code: &str) -> Option<Self> {
    Self::ALL.into_iter().find(|language| language.code() == code)
  }

  /// A language from a locale tag such as `tr-TR`, `tr_TR.UTF-8`, or `en`.
  pub fn from_tag(tag: &str) -> Option<Self> {
    let primary = tag.trim().trim_matches('"').split(['-', '_', '.', '@']).next()?.to_ascii_lowercase();
    Self::from_code(&primary)
  }

  pub(crate) fn index(self) -> u8 {
    match self {
      Self::English => 0,
      Self::Turkish => 1,
    }
  }

  pub(crate) fn from_index(index: u8) -> Self {
    Self::ALL.into_iter().find(|language| language.index() == index).unwrap_or(Self::English)
  }
}

/// The first supported language in the user's preference order, or English when
/// none of them is supported.
pub fn resolve(preferences: &[String]) -> Language {
  preferences.iter().find_map(|tag| Language::from_tag(tag)).unwrap_or(Language::English)
}

/// The language of this computer, from the macOS preferred languages or the
/// locale environment variables elsewhere.
pub fn system_language() -> Language {
  resolve(&system_preferences())
}

fn system_preferences() -> Vec<String> {
  if cfg!(target_os = "macos") {
    if let Ok(output) = Command::new("/usr/bin/defaults").args(["read", "-g", "AppleLanguages"]).output() {
      let tags = parse_apple_languages(&String::from_utf8_lossy(&output.stdout));
      if !tags.is_empty() {
        return tags;
      }
    }
  }
  ["LC_ALL", "LC_MESSAGES", "LANG"].iter().filter_map(|name| std::env::var(name).ok()).collect()
}

/// Reads the property-list style output of `defaults read -g AppleLanguages`,
/// e.g. `(\n    "tr-TR",\n    "en-US"\n)`.
pub(crate) fn parse_apple_languages(output: &str) -> Vec<String> {
  output
    .lines()
    .map(|line| line.trim().trim_end_matches(',').trim_matches('"'))
    .filter(|line| !line.is_empty() && *line != "(" && *line != ")")
    .map(str::to_string)
    .collect()
}
