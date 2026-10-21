use super::{
  components::{green, orange, selection_checkbox, tool_icon, tool_tint},
  Workspace,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants},
    h_flex, v_flex, ActiveTheme, Icon, IconName, Sizable,
  },
  *,
};
use tendkit_model::{ToolCategory, ToolRow};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum CatalogFilter {
  #[default]
  All,
  Installed,
  NotInstalled,
}

impl CatalogFilter {
  pub(super) fn includes(self, tool: &ToolRow) -> bool {
    match self {
      Self::All => true,
      Self::Installed => tool.installed_version().is_some(),
      Self::NotInstalled => tool.scanned && tool.installations.is_empty(),
    }
  }
}

impl Workspace {
  pub(super) fn render_catalog(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    if self.focused_tool.is_some_and(|id| self.visible_tools(cx).iter().any(|tool| tool.id == id)) {
      return v_flex()
        .size_full()
        .child(div().px_5().pt_3().child(
          Button::new("back-to-catalog").ghost().small().icon(IconName::ArrowLeft).label("Back to All Tools").on_click(cx.listener(
            |this, _, _, cx| {
              this.focused_tool = None;
              cx.notify();
            },
          )),
        ))
        .child(div().flex_1().min_h_0().child(self.render_detail(window, cx)))
        .into_any_element();
    }

    let model = self.model.read(cx);
    let installed = model.installed_count();
    let total = model.tools.len();
    let scanning = model.is_scanning;
    let show_progress = model.is_executing || model.run_status.is_some();
    let show_bar = !model.selected.is_empty() || model.is_planning;
    let error = model.error.clone();
    let tools = self.visible_tools(cx);

    let mut filters = h_flex().gap_2();
    for (filter, label) in
      [(CatalogFilter::All, "All tools"), (CatalogFilter::Installed, "Installed"), (CatalogFilter::NotInstalled, "Not installed")]
    {
      let active = self.catalog_filter == filter;
      filters = filters.child(
        Button::new(SharedString::from(format!("catalog-filter-{label}")))
          .ghost()
          .small()
          .label(label)
          .when(active, |button| button.bg(theme.blue.opacity(0.12)).text_color(theme.blue))
          .on_click(cx.listener(move |this, _, _, cx| {
            this.catalog_filter = filter;
            cx.notify();
          })),
      );
    }

    let mut body = v_flex()
      .w_full()
      .max_w(px(1060.))
      .p(px(26.))
      .gap(px(24.))
      .child(
        v_flex()
          .gap_2()
          .child(
            h_flex()
              .gap_3()
              .child(div().text_size(px(26.)).font_weight(FontWeight::SEMIBOLD).child("Build your workspace."))
              .child(div().flex_1())
              .child(
                h_flex()
                  .gap_2()
                  .px_3()
                  .py_2()
                  .rounded(px(20.))
                  .bg(green().opacity(0.08))
                  .text_color(if scanning { theme.muted_foreground } else { green() })
                  .text_size(px(11.))
                  .child(Icon::new(if scanning { IconName::RefreshCw } else { IconName::CircleCheck }).size(px(13.)))
                  .child(if scanning { "Scanning tools…".to_string() } else { format!("{installed} of {total} installed") }),
              ),
          )
          .child(
            div()
              .text_size(px(13.))
              .text_color(theme.muted_foreground)
              .child("Discover the runtimes, package managers, and AI tools for your next project."),
          ),
      )
      .child(filters);

    if let Some(error) = error {
      body = body.child(div().p_3().rounded(px(10.)).bg(orange().opacity(0.08)).text_color(orange()).text_size(px(12.)).child(error));
    }
    if show_progress {
      body = body.child(self.render_progress(cx));
    }
    if tools.is_empty() {
      body = body.child(
        v_flex()
          .items_center()
          .gap_3()
          .py(px(60.))
          .text_color(theme.muted_foreground)
          .child(Icon::new(IconName::Search).size(px(28.)))
          .child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).child("No matching tools"))
          .child(div().text_size(px(12.)).child("Try another search or choose a different filter.")),
      );
    }

    for category in ToolCategory::ALL {
      let members: Vec<_> = tools.iter().filter(|tool| tool.category() == category).collect();
      if members.is_empty() {
        continue;
      }
      let description = match category {
        ToolCategory::PackageManager => "The foundation for your developer tools",
        ToolCategory::JavaScript => "Run, build, and ship for the web",
        ToolCategory::Python => "From quick scripts to data science",
        ToolCategory::Rust => "Build fast, dependable native applications",
        ToolCategory::Ai => "A little more intelligence in your workflow",
      };
      // Equal-width flex rows avoid running the grid sizing algorithm on every scroll frame.
      let mut cards = v_flex().gap_3();
      for pair in members.chunks(2) {
        let mut row = h_flex().items_stretch().gap_3();
        for tool in pair {
          row = row.child(div().flex_1().min_w_0().child(self.catalog_card(tool, cx)));
        }
        if pair.len() == 1 {
          row = row.child(div().flex_1().min_w_0());
        }
        cards = cards.child(row);
      }
      body = body.child(
        v_flex()
          .gap_3()
          .child(
            h_flex()
              .gap_2()
              .child(div().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child(category.title()))
              .child(div().text_size(px(11.)).text_color(theme.muted_foreground).child(format!("{}", members.len())))
              .child(div().flex_1())
              .child(div().text_size(px(11.)).text_color(theme.muted_foreground).child(description)),
          )
          .child(cards),
      );
    }

    v_flex()
      .size_full()
      .child(div().id("tool-catalog").flex_1().min_h_0().overflow_y_scroll().child(h_flex().w_full().justify_center().child(body)))
      .when(show_bar, |this| this.child(self.installation_bar(cx)))
      .into_any_element()
  }

  fn catalog_card(&self, tool: &ToolRow, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let tint = tool_tint(tool.id, cx);
    let id = tool.id;
    let selected = self.model.read(cx).selected.contains(id);
    let (status, color) = if !tool.scanned {
      ("Waiting for scan".to_string(), theme.muted_foreground)
    } else if let Some(version) = tool.installed_version() {
      (format!("Installed · {version}"), green())
    } else if !tool.installations.is_empty() {
      ("Needs attention".to_string(), orange())
    } else {
      ("Not installed".to_string(), theme.muted_foreground)
    };

    v_flex()
      .id(SharedString::from(format!("catalog-{id}")))
      .w_full()
      .h_full()
      .min_w_0()
      .gap(px(14.))
      .p(px(17.))
      .rounded(px(14.))
      .border_1()
      .border_color(if selected { theme.blue.opacity(0.65) } else { theme.border })
      .bg(if selected { theme.blue.opacity(0.06) } else { tint.opacity(0.025) })
      .hover(move |style| style.bg(tint.opacity(0.07)).border_color(tint.opacity(0.4)))
      .child(
        h_flex()
          .gap_3()
          .child(tool_icon(tool, 38., cx))
          .child(
            v_flex()
              .flex_1()
              .min_w_0()
              .gap_1()
              .child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).child(tool.name))
              .child(div().text_size(px(11.)).text_color(theme.muted_foreground).child(tool.subtitle)),
          )
          .child(selection_checkbox(&self.model, tool, false, cx)),
      )
      .child(
        h_flex()
          .gap_2()
          .pt_3()
          .border_t_1()
          .border_color(theme.border.opacity(0.6))
          .child(div().size(px(5.)).rounded_full().bg(color))
          .child(div().text_size(px(11.)).text_color(color).child(status))
          .child(div().flex_1())
          .child(
            Button::new(SharedString::from(format!("details-{id}")))
              .ghost()
              .xsmall()
              .label("Details")
              .icon(IconName::ArrowRight)
              .accessibility_label(format!("View {} details", tool.name))
              .on_click(cx.listener(move |this, _, _, cx| this.focus_tool(id, cx))),
          ),
      )
  }
}
