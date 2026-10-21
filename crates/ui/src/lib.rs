//! The TendKit interface: the workspace window content, its views, and the
//! application-level actions, built with GPUI Kit.

mod assets;
mod catalog;
mod components;
mod opencommit;
mod plan;
mod platform;
mod progress;
mod settings;
mod sidebar;
mod tool_detail;
mod tool_list;

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants},
    h_flex,
    input::{Input, InputEvent, InputState},
    v_flex, ActiveTheme, Disableable, IconName, Sizable, Theme, TitleBar, WindowExt,
  },
  *,
};
use std::time::SystemTime;
use tendkit_model::AppModel;

pub use assets::Assets;

actions!(tendkit, [Quit, ScanAgain, Hide, HideOthers, ShowAll, Minimize, Zoom]);
actions!(workspace, [SelectPrevious, SelectNext]);

const CONTEXT: &str = "Workspace";
/// Shared toolbar height; the sidebar starts below the same band.
pub const TOOLBAR_HEIGHT: Pixels = px(60.);

/// Follows the system appearance and uses the macOS blue accent for primary
/// controls instead of GPUI Kit's default black.
fn apply_theme(window: &mut Window, cx: &mut App) {
  Theme::sync_system_appearance(Some(window), cx);
  Theme::update(cx, |theme| {
    let accent = if theme.is_dark() { hsla(211. / 360., 1., 0.55, 1.) } else { hsla(211. / 360., 1., 0.5, 1.) };
    theme.blue = accent;
    theme.primary = accent;
    theme.primary_hover = accent.opacity(0.9);
    theme.primary_active = accent.opacity(0.8);
    theme.primary_foreground = white();
    theme.button_primary = accent;
    theme.button_primary_hover = accent.opacity(0.9);
    theme.button_primary_active = accent.opacity(0.8);
    theme.button_primary_foreground = white();
    theme.ring = accent.opacity(0.6);
  });
  window.refresh();
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Destination {
  AllTools,
  Installed,
  Settings,
}

impl Destination {
  pub const ALL: [Self; 3] = [Self::AllTools, Self::Installed, Self::Settings];

  pub fn title(self) -> &'static str {
    match self {
      Self::AllTools => "All Tools",
      Self::Installed => "Installed",
      Self::Settings => "Settings",
    }
  }

  pub fn includes(self, tool: &tendkit_model::ToolRow) -> bool {
    match self {
      Self::AllTools | Self::Settings => true,
      Self::Installed => tool.installed_version().is_some(),
    }
  }
}

pub struct Workspace {
  model: Entity<AppModel>,
  destination: Destination,
  catalog_filter: catalog::CatalogFilter,
  search: Entity<InputState>,
  focused_tool: Option<&'static str>,
  /// The tool and scan state the latest version was last requested for.
  latest_request: Option<(&'static str, bool, Option<SystemTime>)>,
  focus_handle: FocusHandle,
  _subscriptions: Vec<Subscription>,
}

impl Workspace {
  pub fn new(model: Entity<AppModel>, window: &mut Window, cx: &mut Context<Self>) -> Self {
    cx.bind_keys([KeyBinding::new("up", SelectPrevious, Some(CONTEXT)), KeyBinding::new("down", SelectNext, Some(CONTEXT))]);
    apply_theme(window, cx);
    let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search tools"));
    let subscriptions = vec![
      cx.observe_window_appearance(window, |_, window, cx| apply_theme(window, cx)),
      cx.subscribe(&search, |_, _, event: &InputEvent, cx| {
        if matches!(event, InputEvent::Change) {
          cx.notify();
        }
      }),
      cx.observe_in(&model, window, |this, model, window, cx| {
        if model.read(cx).show_plan && !window.has_active_dialog(cx) {
          this.open_plan(window, cx);
        }
        cx.notify();
      }),
    ];
    let focus_handle = cx.focus_handle();
    focus_handle.focus(window, cx);
    Self {
      model,
      destination: Destination::AllTools,
      catalog_filter: catalog::CatalogFilter::default(),
      search,
      focused_tool: None,
      latest_request: None,
      focus_handle,
      _subscriptions: subscriptions,
    }
  }

  fn query(&self, cx: &App) -> String {
    self.search.read(cx).value().trim().to_lowercase()
  }

  /// Tools shown for the current destination and search, in category order.
  pub(crate) fn visible_tools(&self, cx: &App) -> Vec<tendkit_model::ToolRow> {
    let query = self.query(cx);
    let model = self.model.read(cx);
    tendkit_model::ToolCategory::ALL
      .iter()
      .flat_map(|category| model.tools.iter().filter(move |t| t.category() == *category))
      .filter(|tool| {
        self.destination.includes(tool) && (self.destination != Destination::AllTools || self.catalog_filter.includes(tool)) && {
          query.is_empty() || {
            let haystack = format!("{} {} {}", tool.name, tool.subtitle, tool.primary().map(|i| i.source.as_str()).unwrap_or(""));
            haystack.to_lowercase().contains(&query)
          }
        }
      })
      .cloned()
      .collect()
  }

  fn navigate(&mut self, destination: Destination, cx: &mut Context<Self>) {
    self.destination = destination;
    self.focused_tool = None;
    cx.notify();
  }

  fn focus_tool(&mut self, id: &'static str, cx: &mut Context<Self>) {
    self.focused_tool = Some(id);
    cx.notify();
  }

  fn move_selection(&mut self, offset: isize, cx: &mut Context<Self>) {
    let ids: Vec<_> = self.visible_tools(cx).iter().map(|t| t.id).collect();
    if ids.is_empty() || self.destination == Destination::Settings {
      return;
    }
    let index = self.focused_tool.and_then(|id| ids.iter().position(|i| *i == id));
    let next = match (index, offset < 0) {
      (Some(i), true) => i.saturating_sub(1),
      (None, true) => ids.len() - 1,
      (Some(i), false) => (i + 1).min(ids.len() - 1),
      (None, false) => 0,
    };
    self.focus_tool(ids[next], cx);
  }

  /// Checks the focused tool's latest version when the tool or scan state changes.
  fn request_latest_version(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let Some(id) = self.focused_tool else { return };
    let model = self.model.read(cx);
    let key = (id, model.is_scanning, model.last_scan);
    if self.latest_request == Some(key) {
      return;
    }
    self.latest_request = Some(key);
    let model = self.model.clone();
    cx.defer_in(window, move |_, _, cx| model.update(cx, |model, cx| model.check_latest_version(id, false, cx)));
  }

  /// The toolbar to the right of the sidebar: title, search, and Scan Again.
  fn toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let subtitle = match self.destination {
      Destination::Settings => "Preferences".to_string(),
      _ => format!("{} tools", self.visible_tools(cx).len()),
    };
    let busy = self.model.read(cx).busy();
    TitleBar::new().h(TOOLBAR_HEIGHT).pl_4().bg(theme.background).border_color(theme.border).child(
      h_flex()
        .w_full()
        .h_full()
        .items_center()
        .pr_3()
        .gap_3()
        .child(
          v_flex()
            .child(
              div()
                .text_size(px(15.))
                .line_height(px(18.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.foreground.opacity(0.85))
                .child(self.destination.title()),
            )
            .child(div().text_size(px(11.)).line_height(px(13.)).text_color(theme.muted_foreground).child(subtitle)),
        )
        .child(div().flex_1())
        .when(self.destination != Destination::Settings, |this| {
          this
            .child(div().w(px(220.)).child(Input::new(&self.search).small().prefix(component::Icon::new(IconName::Search).small()).cleanable(true)))
            .child(
              Button::new("scan-again")
                .ghost()
                .small()
                .icon(IconName::RefreshCw)
                .tooltip("Clear cached versions and scan your developer environment")
                .disabled(busy)
                .on_click(|_, window, cx| window.dispatch_action(Box::new(ScanAgain), cx)),
            )
        }),
    )
  }
}

impl Render for Workspace {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    self.request_latest_version(window, cx);
    let theme = cx.theme().clone();
    let content = if self.destination == Destination::Settings {
      self.render_settings(cx).into_any_element()
    } else if self.destination == Destination::AllTools {
      self.render_catalog(window, cx).into_any_element()
    } else {
      h_flex().size_full().child(self.render_tool_list(cx)).child(self.render_detail(window, cx)).into_any_element()
    };
    h_flex()
      .id("workspace")
      .key_context(CONTEXT)
      .track_focus(&self.focus_handle)
      .on_action(cx.listener(|this, _: &SelectPrevious, _, cx| this.move_selection(-1, cx)))
      .on_action(cx.listener(|this, _: &SelectNext, _, cx| this.move_selection(1, cx)))
      .size_full()
      .bg(theme.background)
      .text_color(theme.foreground)
      .child(self.render_sidebar(cx))
      .child(v_flex().flex_1().min_w_0().h_full().child(self.toolbar(cx)).child(div().flex_1().min_h_0().w_full().child(content)))
  }
}
