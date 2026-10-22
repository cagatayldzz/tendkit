//! Interface languages. English is the development language and the fallback;
//! every other language is a `locales/<code>.txt` file of `key = value` lines.
//!
//! Text is looked up with [`t`] (plain) or [`tf`] (with `{name}` placeholders).
//! A key missing from the active language falls back to English, then to the key
//! itself, so a gap never breaks the interface.

mod catalog;
mod language;

pub use language::{resolve, system_language, Language};

use std::sync::atomic::{AtomicU8, Ordering};

static CURRENT: AtomicU8 = AtomicU8::new(0);

/// Switches the language used by [`t`] and [`tf`].
pub fn set_language(language: Language) {
  CURRENT.store(language.index(), Ordering::Relaxed);
}

pub fn language() -> Language {
  Language::from_index(CURRENT.load(Ordering::Relaxed))
}

/// The text for `key` in the active language.
pub fn t(key: &str) -> String {
  catalog::lookup(language(), key).to_string()
}

/// Like [`t`], with each `{name}` placeholder replaced by its argument.
pub fn tf(key: &str, args: &[(&str, &str)]) -> String {
  let mut text = catalog::lookup(language(), key).to_string();
  for (name, value) in args {
    text = text.replace(&format!("{{{name}}}"), value);
  }
  text
}

#[cfg(test)]
#[path = "../tests/localization.rs"]
mod tests;
