use super::{apply, contrast, luminance};
use gpui_kit::{black, component::ThemeColor, white};

fn themed(dark: bool) -> ThemeColor {
  let mut colors = ThemeColor::default();
  apply(&mut colors, dark);
  colors
}

#[test]
fn text_pairs_stay_readable_in_both_themes() {
  for (name, dark) in [("light", false), ("dark", true)] {
    let c = themed(dark);
    let pairs = [
      ("text on page", c.foreground, c.background),
      ("text on card", c.foreground, c.group_box),
      ("muted text on page", c.muted_foreground, c.background),
      ("muted text on card", c.muted_foreground, c.group_box),
      ("sidebar text", c.sidebar_foreground, c.sidebar),
      ("muted text on sidebar", c.muted_foreground, c.sidebar),
      ("status bar text", c.foreground, c.status_bar),
      ("selected sidebar row", c.sidebar_accent_foreground, c.sidebar_accent),
      // Nested buttons, such as the submenu caret, keep the normal text colour.
      ("caret on selected sidebar row", c.foreground, c.sidebar_accent),
      ("accent text on page", c.blue, c.background),
      ("link on page", c.link, c.background),
      ("primary button", c.button_primary_foreground, c.button_primary),
      ("primary", c.primary_foreground, c.primary),
      ("success tag", c.success_foreground, c.success),
      ("danger tag", c.danger_foreground, c.danger),
      ("warning tag", c.warning_foreground, c.warning),
      ("info tag", c.info_foreground, c.info),
    ];
    for (label, foreground, background) in pairs {
      let ratio = contrast(foreground, background);
      assert!(ratio >= 4.5, "{name} theme: {label} has contrast {ratio:.2}, below 4.5");
    }
  }
}

#[test]
fn surfaces_are_distinguishable_from_the_page_in_the_dark_theme() {
  let c = themed(true);
  assert!(luminance(c.sidebar) < luminance(c.background));
  assert!(luminance(c.group_box) > luminance(c.background));
  assert!(contrast(c.border, c.background) >= 1.2);
}

#[test]
fn contrast_matches_the_wcag_reference_values() {
  assert!((contrast(white(), black()) - 21.0).abs() < 0.01);
  assert!((contrast(white(), white()) - 1.0).abs() < 0.01);
}
