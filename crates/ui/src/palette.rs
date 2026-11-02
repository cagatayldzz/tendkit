//! The GitHub Desktop colour palette, applied over GPUI Kit's theme tokens.

use gpui_kit::{component::ThemeColor, *};

fn hex(value: u32) -> Hsla {
  rgb(value).into()
}

/// Black or white, whichever reads better on `background`.
fn on(background: Hsla) -> Hsla {
  let dark = hex(0x1b1f23);
  if contrast(dark, background) >= contrast(white(), background) {
    dark
  } else {
    white()
  }
}

/// WCAG relative luminance of a colour.
fn luminance(color: Hsla) -> f32 {
  let rgba: Rgba = color.into();
  let channel = |value: f32| if value <= 0.03928 { value / 12.92 } else { ((value + 0.055) / 1.055).powf(2.4) };
  0.2126 * channel(rgba.r) + 0.7152 * channel(rgba.g) + 0.0722 * channel(rgba.b)
}

/// WCAG contrast ratio between two colours, from 1 to 21.
fn contrast(a: Hsla, b: Hsla) -> f32 {
  let (a, b) = (luminance(a), luminance(b));
  (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// The handful of colours every other token is derived from.
struct Palette {
  background: Hsla,
  /// Toolbar, sidebar, and other chrome above the content.
  chrome: Hsla,
  /// Cards and grouped content, one step above the page background.
  surface: Hsla,
  /// Raised controls and hovered rows.
  raised: Hsla,
  border: Hsla,
  foreground: Hsla,
  muted_foreground: Hsla,
  accent: Hsla,
  /// The accent as text or a thin line on the page background.
  accent_text: Hsla,
  accent_hover: Hsla,
  accent_active: Hsla,
  success: Hsla,
  danger: Hsla,
  warning: Hsla,
  selection: Hsla,
  /// Selected sidebar row. Nested controls such as the submenu caret keep the
  /// normal foreground colour, so the row must stay readable under it.
  sidebar_selected: Hsla,
  sidebar_selected_foreground: Hsla,
}

impl Palette {
  fn dark() -> Self {
    Self {
      background: hex(0x1f2428),
      chrome: hex(0x171b1f),
      surface: hex(0x272d34),
      raised: hex(0x323941),
      border: hex(0x30363d),
      foreground: hex(0xe1e4e8),
      muted_foreground: hex(0x959da5),
      accent: hex(0x2a63d6),
      accent_text: hex(0x58a6ff),
      accent_hover: hex(0x3470e8),
      accent_active: hex(0x2253b8),
      success: hex(0x34d058),
      danger: hex(0xf97583),
      warning: hex(0xffd33d),
      selection: hex(0x2a63d6).opacity(0.45),
      sidebar_selected: hex(0x2557c0),
      sidebar_selected_foreground: white(),
    }
  }

  fn light() -> Self {
    Self {
      background: hex(0xffffff),
      chrome: hex(0xf6f8fa),
      surface: hex(0xf6f8fa),
      raised: hex(0xeaecef),
      border: hex(0xe1e4e8),
      foreground: hex(0x24292e),
      muted_foreground: hex(0x586069),
      accent: hex(0x0366d6),
      accent_text: hex(0x0366d6),
      accent_hover: hex(0x035fc7),
      accent_active: hex(0x0256b4),
      success: hex(0x1a7f37),
      danger: hex(0xcb2431),
      warning: hex(0xdbab09),
      selection: hex(0x0366d6).opacity(0.25),
      sidebar_selected: hex(0xdbe9f9),
      sidebar_selected_foreground: hex(0x044289),
    }
  }

  fn apply(&self, colors: &mut ThemeColor) {
    let white = white();
    colors.background = self.background;
    colors.foreground = self.foreground;
    colors.border = self.border;
    colors.input = self.muted_foreground.opacity(0.45);
    colors.muted = self.surface;
    colors.muted_foreground = self.muted_foreground;
    colors.accent = self.raised;
    colors.accent_foreground = self.foreground;
    colors.popover = self.chrome;
    colors.popover_foreground = self.foreground;
    colors.group_box = self.surface;
    colors.group_box_foreground = self.foreground;
    colors.description_list_label = self.chrome;
    colors.description_list_label_foreground = self.muted_foreground;
    colors.selection = self.selection;
    colors.caret = self.foreground;
    colors.link = self.accent_text;
    colors.link_hover = self.accent_hover;
    colors.link_active = self.accent_active;
    colors.ring = self.accent.opacity(0.6);
    colors.overlay = hsla(0., 0., 0., 0.5);
    colors.window_border = self.border;

    colors.primary = self.accent;
    colors.primary_hover = self.accent_hover;
    colors.primary_active = self.accent_active;
    colors.primary_foreground = on(self.accent);
    colors.button_primary = self.accent;
    colors.button_primary_hover = self.accent_hover;
    colors.button_primary_active = self.accent_active;
    colors.button_primary_foreground = on(self.accent);
    colors.secondary = self.raised;
    colors.secondary_hover = self.raised.opacity(0.85);
    colors.secondary_active = self.raised.opacity(0.7);
    colors.secondary_foreground = self.foreground;
    colors.button_secondary = self.raised;
    colors.button_secondary_hover = self.raised.opacity(0.85);
    colors.button_secondary_active = self.raised.opacity(0.7);
    colors.button_secondary_foreground = self.foreground;

    colors.success = self.success;
    colors.success_hover = self.success.opacity(0.9);
    colors.success_active = self.success.opacity(0.8);
    colors.success_foreground = on(self.success);
    colors.danger = self.danger;
    colors.danger_hover = self.danger.opacity(0.9);
    colors.danger_active = self.danger.opacity(0.8);
    colors.danger_foreground = on(self.danger);
    colors.warning = self.warning;
    colors.warning_hover = self.warning.opacity(0.9);
    colors.warning_active = self.warning.opacity(0.8);
    colors.warning_foreground = on(self.warning);
    colors.info = self.accent;
    colors.info_hover = self.accent_hover;
    colors.info_active = self.accent_active;
    colors.info_foreground = on(self.accent);
    colors.blue = self.accent_text;
    colors.green = self.success;
    colors.red = self.danger;
    colors.yellow = self.warning;

    colors.sidebar = self.chrome;
    colors.sidebar_foreground = self.foreground;
    colors.sidebar_border = self.border;
    colors.sidebar_accent = self.sidebar_selected;
    colors.sidebar_accent_foreground = self.sidebar_selected_foreground;
    colors.sidebar_primary = self.accent;
    colors.sidebar_primary_foreground = white;
    colors.title_bar = self.background;
    colors.title_bar_border = self.border;
    colors.status_bar = self.chrome;
    colors.status_bar_border = self.border;
    colors.tab_bar = self.chrome;
    colors.tab = self.chrome;
    colors.tab_foreground = self.muted_foreground;
    colors.tab_active = self.background;
    colors.tab_active_foreground = self.foreground;

    colors.list = self.background;
    colors.list_even = self.background;
    colors.list_head = self.chrome;
    colors.list_hover = self.raised.opacity(0.6);
    colors.list_active = self.accent.opacity(0.2);
    colors.list_active_border = self.accent;
    colors.table = self.background;
    colors.table_even = self.background;
    colors.table_head = self.chrome;
    colors.table_head_foreground = self.muted_foreground;
    colors.table_hover = self.raised.opacity(0.6);
    colors.table_active = self.accent.opacity(0.2);
    colors.table_active_border = self.accent;
    colors.table_row_border = self.border;

    colors.scrollbar = hsla(0., 0., 0., 0.);
    colors.scrollbar_thumb = self.muted_foreground.opacity(0.4);
    colors.scrollbar_thumb_hover = self.muted_foreground.opacity(0.65);
    colors.progress_bar = self.accent;
    colors.switch = self.muted_foreground.opacity(0.35);
    colors.switch_thumb = white;
    colors.slider_bar = self.accent;
    colors.slider_thumb = white;
  }
}

/// Applies the dark or light GitHub Desktop palette to the theme colours.
pub fn apply(colors: &mut ThemeColor, dark: bool) {
  if dark {
    Palette::dark().apply(colors);
  } else {
    Palette::light().apply(colors);
  }
}

#[cfg(test)]
#[path = "../tests/palette.rs"]
mod tests;
