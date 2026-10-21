use super::{
  components::{caption, green, orange, selection_checkbox, status_tag, tool_icon, tool_tint},
  Workspace,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    alert::Alert,
    button::{Button, ButtonVariants},
    h_flex, v_flex, ActiveTheme, Disableable, Icon, IconName, Sizable,
  },
  *,
};
use std::collections::BTreeSet;
use tendkit_i18n::{t, tf};
use tendkit_model::{AppModel, ToolCategory, ToolRow};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum CatalogFilter {
  #[default]
  All,
  Installed,
  Updates,
  NotInstalled,
}

impl CatalogFilter {
  pub(super) fn includes(self, tool: &ToolRow, model: &AppModel) -> bool {
    match self {
      Self::All => true,
      Self::Installed => tool.installed_version().is_some(),
      Self::Updates => model.can_update(tool),
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
        .child(div().px_5().pt_3().child(Button::new("back-to-catalog").ghost().small().icon(IconName::ArrowLeft).label(t("catalog.back")).on_click(
          cx.listener(|this, _, _, cx| {
            this.focused_tool = None;
            cx.notify();
          }),
        )))
        .child(div().flex_1().min_h_0().child(self.render_detail(window, cx)))
        .into_any_element();
    }

    let model = self.model.read(cx);
    let installed = model.installed_count();
    let total = model.tools.len();
    let not_installed = model.tools.iter().filter(|t| t.scanned && t.installations.is_empty()).count();
    let updates = model.update_count();
    let updatable = model.updatable_ids().len();
    let checking = model.is_checking_updates();
    let busy = model.busy();
    let scanning = model.is_scanning;
    let last_check = model.last_update_check.map(crate::platform::local_time);
    let show_progress = model.is_executing || model.run_status.is_some();
    let show_bar = !model.selected.is_empty() || model.is_planning;
    let error = model.error.clone();
    let tools = self.visible_tools(cx);

    let mut filters = h_flex().gap_2();
    for (filter, label) in [
      (CatalogFilter::All, "filter.all"),
      (CatalogFilter::Installed, "nav.installed"),
      (CatalogFilter::Updates, "nav.updates"),
      (CatalogFilter::NotInstalled, "status.not_installed"),
    ] {
      let active = self.catalog_filter == filter;
      filters = filters.child(
        Button::new(SharedString::from(format!("catalog-filter-{label}")))
          .ghost()
          .small()
          .label(t(label))
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
        h_flex()
          .gap_3()
          .child(self.stat_tile("nav.installed", format!("{installed}/{total}"), green(), CatalogFilter::Installed, cx))
          .child(self.stat_tile(
            "nav.updates",
            updates.to_string(),
            if updates > 0 { orange() } else { theme.muted_foreground },
            CatalogFilter::Updates,
            cx,
          ))
          .child(self.stat_tile("status.not_installed", not_installed.to_string(), theme.muted_foreground, CatalogFilter::NotInstalled, cx)),
      )
      .child(
        h_flex()
          .gap_3()
          .child(
            h_flex()
              .gap_2()
              .text_size(px(12.))
              .text_color(theme.muted_foreground)
              .child(Icon::new(if scanning || checking { IconName::RefreshCw } else { IconName::CircleCheck }).size(px(13.)))
              .child(if scanning {
                t("activity.scanning")
              } else if checking {
                t("catalog.checking")
              } else if let Some(time) = last_check {
                tf("catalog.checked_at", &[("time", &time)])
              } else {
                t("catalog.not_checked")
              }),
          )
          .child(div().flex_1())
          .child(
            Button::new("check-updates")
              .ghost()
              .small()
              .icon(IconName::RefreshCw)
              .label(t("action.check_updates"))
              .disabled(busy || checking)
              .on_click(cx.listener(|this, _, _, cx| {
                this.model.update(cx, |model, cx| model.check_all_latest(true, cx));
              })),
          )
          .when(updatable > 0, |this| {
            this.child(
              Button::new("update-all")
                .primary()
                .small()
                .icon(IconName::ArrowUp)
                .label(tf("action.update_all_count", &[("count", &updatable.to_string())]))
                .disabled(busy)
                .on_click(cx.listener(|this, _, _, cx| {
                  this.model.update(cx, |model, cx| model.update_all(cx));
                })),
            )
          }),
      )
      .child(filters);

    if let Some(error) = error {
      body = body.child(Alert::error("catalog-error", error));
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
          .child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).child(t("catalog.no_match")))
          .child(div().text_size(px(12.)).child(t("catalog.no_match_hint"))),
      );
    }

    for category in ToolCategory::ALL {
      let members: Vec<_> = tools.iter().filter(|tool| tool.category() == category).collect();
      if members.is_empty() {
        continue;
      }
      let description = category.description();
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
    let model = self.model.read(cx);
    let tag = status_tag(model, tool);
    let version = tool.installed_version().map(|v| format!("v{v}"));
    let has_update = model.can_update(tool);
    let can_update = has_update && !model.busy();

    v_flex()
      .id(SharedString::from(format!("catalog-{id}")))
      .w_full()
      .h_full()
      .min_w_0()
      .gap(px(14.))
      .p(px(17.))
      .rounded(px(14.))
      .border_1()
      .border_color(if selected {
        theme.blue.opacity(0.65)
      } else if has_update {
        orange().opacity(0.5)
      } else {
        theme.border
      })
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
              .child(div().text_size(px(11.)).text_color(theme.muted_foreground).child(tool.subtitle())),
          )
          .child(selection_checkbox(&self.model, tool, false, cx)),
      )
      .child(
        h_flex()
          .gap_2()
          .pt_3()
          .border_t_1()
          .border_color(theme.border.opacity(0.6))
          .child(tag)
          .child(div().min_w_0().truncate().child(caption(version.unwrap_or_default(), cx)))
          .child(div().flex_1())
          .when(can_update, |this| {
            let update_model = self.model.clone();
            this.child(
              Button::new(SharedString::from(format!("update-{id}")))
                .primary()
                .xsmall()
                .label(t("action.update"))
                .accessibility_label(tf("action.update_accessibility", &[("name", tool.name)]))
                .on_click(move |_, _, cx| update_model.update(cx, |model, cx| model.prepare(Some(BTreeSet::from([id.to_string()])), cx))),
            )
          })
          .child(
            Button::new(SharedString::from(format!("details-{id}")))
              .ghost()
              .xsmall()
              .label(t("action.details"))
              .icon(IconName::ArrowRight)
              .accessibility_label(tf("action.details_accessibility", &[("name", tool.name)]))
              .on_click(cx.listener(move |this, _, _, cx| this.focus_tool(id, cx))),
          ),
      )
  }

  /// A clickable summary figure that filters the catalog.
  fn stat_tile(&self, label_key: &'static str, value: String, color: Hsla, filter: CatalogFilter, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let active = self.catalog_filter == filter;
    v_flex()
      .id(SharedString::from(format!("stat-{label_key}")))
      .flex_1()
      .gap(px(4.))
      .px_4()
      .py_3()
      .rounded(px(12.))
      .cursor_pointer()
      .border_1()
      .border_color(if active { color.opacity(0.55) } else { theme.border })
      .bg(if active { color.opacity(0.07) } else { theme.foreground.opacity(0.025) })
      .hover(move |style| style.bg(color.opacity(0.08)))
      .on_click(cx.listener(move |this, _, _, cx| {
        this.catalog_filter = if this.catalog_filter == filter { CatalogFilter::All } else { filter };
        cx.notify();
      }))
      .child(div().text_size(px(22.)).font_weight(FontWeight::SEMIBOLD).text_color(color).child(value))
      .child(caption(t(label_key), cx))
  }
}
