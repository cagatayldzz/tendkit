use crate::Language;
use std::{collections::HashMap, sync::OnceLock};

const ENGLISH: &str = include_str!("../locales/en.txt");
const TURKISH: &str = include_str!("../locales/tr.txt");

type Messages = HashMap<&'static str, String>;

fn source(language: Language) -> &'static str {
  match language {
    Language::English => ENGLISH,
    Language::Turkish => TURKISH,
  }
}

/// Parses `key = value` lines; blank lines and `#` comments are skipped, and `\n`
/// in a value is a line break.
pub(crate) fn parse(text: &'static str) -> Messages {
  text
    .lines()
    .map(str::trim)
    .filter(|line| !line.is_empty() && !line.starts_with('#'))
    .filter_map(|line| line.split_once('='))
    .map(|(key, value)| (key.trim(), value.trim().replace("\\n", "\n")))
    .collect()
}

fn messages(language: Language) -> &'static Messages {
  static CATALOGS: [OnceLock<Messages>; 2] = [OnceLock::new(), OnceLock::new()];
  CATALOGS[usize::from(language.index())].get_or_init(|| parse(source(language)))
}

/// The text for `key`, falling back to English and then to the key itself.
pub(crate) fn lookup(language: Language, key: &str) -> &str {
  messages(language).get(key).or_else(|| messages(Language::English).get(key)).map(String::as_str).unwrap_or(key)
}
