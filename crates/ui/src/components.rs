use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{checkbox::Checkbox, ActiveTheme, Disableable, Icon, Sizable},
  *,
};
use gpui_kit_assets::IconName as Lucide;
use tendkit_model::{AppModel, ToolRow, UpdateStatus};

pub fn orange() -> Hsla {
  hsla(28. / 360., 0.92, 0.52, 1.)
}
pub fn green() -> Hsla {
  hsla(142. / 360., 0.62, 0.42, 1.)
}

pub fn tool_tint(id: &str, cx: &App) -> Hsla {
  match id {
    "homebrew" | "claude" => orange(),
    "node" => green(),
    "python" => hsla(210. / 360., 0.65, 0.56, 1.),
    "rust" => hsla(18. / 360., 0.7, 0.55, 1.),
    "npm" => hsla(0., 0.72, 0.52, 1.),
    "bun" => hsla(45. / 360., 0.92, 0.48, 1.),
    "opencommit" => hsla(270. / 360., 0.6, 0.58, 1.),
    "codex" => cx.theme().foreground,
    _ => cx.theme().muted_foreground,
  }
}

pub fn tool_icon_name(id: &str) -> Lucide {
  match id {
    "homebrew" => Lucide::Package,
    "node" => Lucide::Hexagon,
    "python" => Lucide::Code,
    "rust" => Lucide::Settings,
    "npm" => Lucide::Layers,
    "bun" => Lucide::Zap,
    "codex" => Lucide::Terminal,
    "claude" => Lucide::Asterisk,
    "opencommit" => Lucide::GitBranch,
    _ => Lucide::Wrench,
  }
}

pub fn tool_icon(tool: &ToolRow, size: f32, cx: &App) -> impl IntoElement {
  let tint = tool_tint(tool.id, cx);
  div()
    .flex_none()
    .size(px(size))
    .rounded(px(size * 0.24))
    .bg(tint.opacity(0.09))
    .flex()
    .items_center()
    .justify_center()
    .child(Icon::new(tool_icon_name(tool.id)).size(px(size * 0.5)).text_color(tint))
}

pub fn selection_checkbox(model: &Entity<AppModel>, tool: &ToolRow, label: bool, cx: &App) -> impl IntoElement {
  let state = model.read(cx);
  let id = tool.id;
  let checkbox_model = model.clone();
  Checkbox::new(SharedString::from(format!("select-{id}")))
    .checked(state.selected.contains(id))
    .disabled(state.busy() || !state.can_manage(tool))
    .small()
    .when(label, |this| this.label("Include in installation"))
    .accessibility_label(format!("Select {} for installation", tool.name))
    .tooltip(if id == "npm" { "npm is managed with Node.js".to_string() } else { format!("Include {} in the installation plan", tool.name) })
    .on_click(move |checked, _, cx| checkbox_model.update(cx, |model, cx| model.toggle_selected(id, *checked, cx)))
}

/// A rounded card background used for grouped details.
pub fn card(cx: &App) -> Div {
  div().rounded(px(12.)).bg(cx.theme().foreground.opacity(0.035))
}

pub fn caption(text: impl Into<SharedString>, cx: &App) -> Div {
  div().text_size(px(11.)).text_color(cx.theme().muted_foreground).child(text.into())
}

pub fn mono(text: impl Into<SharedString>) -> Div {
  div().font_family("Menlo").child(text.into())
}

/// One-line status for a tool: its install state and, for installed tools, whether
/// a newer version is available. Returns the text and its color.
pub fn status_summary(model: &AppModel, tool: &ToolRow, cx: &App) -> (String, Hsla) {
  let muted = cx.theme().muted_foreground;
  if !tool.scanned {
    return ("Waiting for scan".into(), muted);
  }
  let Some(version) = tool.installed_version() else {
    return if tool.installations.is_empty() { ("Not installed".into(), muted) } else { ("Needs attention".into(), orange()) };
  };
  match model.update_status(tool) {
    // npm ships with Node.js, so it has no update action of its own.
    Some(UpdateStatus::Available { installed, latest }) if tool.id == "npm" => (format!("Updates with Node.js · {installed} → {latest}"), orange()),
    Some(UpdateStatus::Available { installed, latest }) => (format!("Update available · {installed} → {latest}"), orange()),
    Some(UpdateStatus::UpToDate) => (format!("Up to date · {version}"), green()),
    Some(UpdateStatus::Checking) => (format!("Installed · {version} · Checking for updates…"), muted),
    _ => (format!("Installed · {version}"), green()),
  }
}

/// A compact orange pill, e.g. the update count on a navigation card.
pub fn pill(text: impl Into<SharedString>, color: Hsla) -> Div {
  div()
    .px(px(7.))
    .py(px(2.))
    .rounded_full()
    .bg(color.opacity(0.14))
    .text_color(color)
    .text_size(px(10.))
    .font_weight(FontWeight::SEMIBOLD)
    .child(text.into())
}
