use super::*;
use crate::{catalog::parse, language::parse_apple_languages};
use std::collections::BTreeSet;

fn placeholders(text: &str) -> BTreeSet<String> {
  text.split('{').skip(1).filter_map(|part| part.split_once('}')).map(|(name, _)| name.to_string()).collect()
}

#[test]
fn every_language_has_the_same_keys_and_placeholders() {
  let english = parse(include_str!("../locales/en.txt"));
  let turkish = parse(include_str!("../locales/tr.txt"));
  let missing: Vec<_> = english.keys().filter(|key| !turkish.contains_key(*key)).collect();
  let extra: Vec<_> = turkish.keys().filter(|key| !english.contains_key(*key)).collect();
  assert!(missing.is_empty(), "missing from tr.txt: {missing:?}");
  assert!(extra.is_empty(), "not in en.txt: {extra:?}");
  for (key, text) in &english {
    assert!(!text.is_empty() && !turkish[key].is_empty(), "{key} is empty");
    assert_eq!(placeholders(text), placeholders(&turkish[key]), "placeholders differ for {key}");
  }
}

#[test]
fn locale_files_have_no_duplicate_keys() {
  for text in [include_str!("../locales/en.txt"), include_str!("../locales/tr.txt")] {
    let lines = text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).count();
    assert_eq!(lines, parse(text).len());
  }
}

#[test]
fn picks_the_first_supported_preferred_language() {
  let tags = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
  assert_eq!(resolve(&tags(&["tr-TR", "en-US"])), Language::Turkish);
  assert_eq!(resolve(&tags(&["de-DE", "tr", "en"])), Language::Turkish);
  assert_eq!(resolve(&tags(&["en-GB", "tr-TR"])), Language::English);
  assert_eq!(resolve(&tags(&["ja-JP", "de-DE"])), Language::English);
  assert_eq!(resolve(&[]), Language::English);
  assert_eq!(resolve(&tags(&["tr_TR.UTF-8"])), Language::Turkish);
}

#[test]
fn reads_apple_language_output() {
  assert_eq!(parse_apple_languages("(\n    \"tr-TR\",\n    \"en-US\"\n)\n"), ["tr-TR", "en-US"]);
}

#[test]
fn languages_round_trip_through_their_codes() {
  for language in Language::ALL {
    assert_eq!(Language::from_code(language.code()), Some(language));
  }
  assert_eq!(Language::from_code("xx"), None);
}

#[test]
fn unknown_keys_fall_back_to_the_key() {
  assert_eq!(t("no.such.key"), "no.such.key");
}

#[test]
fn placeholders_are_replaced() {
  set_language(Language::English);
  assert_eq!(tf("test.greeting", &[("name", "Ada")]), "Hello, Ada");
  set_language(Language::Turkish);
  assert_eq!(tf("test.greeting", &[("name", "Ada")]), "Merhaba, Ada");
  set_language(Language::English);
}
