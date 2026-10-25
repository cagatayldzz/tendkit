use super::{
  components::{caption, green, orange, selection_checkbox, status_tag, tool_icon, tool_tint},
  Destination, Workspace,
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
use tendkit_model::{AppModel, ToolCategory, ToolRow, UpdateStatus};

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
      if self.destination == Destination::Installed {
        return v_flex()
          .size_full()
          .child(div().px_5().py_2().border_b_1().border_color(theme.border).child(
            Button::new("back-to-installed").ghost().small().icon(IconName::ArrowLeft).label(t("catalog.back_installed")).on_click(cx.listener(
              |this, _, _, cx| {
                this.focused_tool = None;
                cx.notify();
              },
            )),
          ))
          .child(h_flex().flex_1().min_h_0().w_full().child(self.render_tool_list(cx)).child(self.render_detail(window, cx)))
          .into_any_element();
      }
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

    let mut filters = h_flex().gap_2().flex_wrap();
    for (filter, label) in [
      (CatalogFilter::All, "filter.all"),
      (CatalogFilter::Installed, "nav.installed"),
      (CatalogFilter::Updates, "nav.updates"),
      (CatalogFilter::NotInstalled, "status.not_installed"),
    ] {
      if self.destination == Destination::Installed && matches!(filter, CatalogFilter::Installed | CatalogFilter::NotInstalled) {
        continue;
      }
      let active = self.catalog_filter == filter;
      filters = filters.child(
        Button::new(SharedString::from(format!("catalog-filter-{label}")))
          .ghost()
          .small()
          .label(t(if self.destination == Destination::Installed && filter == CatalogFilter::All { "catalog.all_installed" } else { label }))
          .when(active, |button| button.bg(theme.blue.opacity(0.12)).text_color(theme.blue))
          .on_click(cx.listener(move |this, _, _, cx| {
            this.catalog_filter = filter;
            cx.notify();
          })),
      );
    }

    let mut body = v_flex()
      .w_full()
      .max_w(px(1200.))
      .p_5()
      .gap_4()
      .child(
        h_flex()
          .flex_wrap()
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
      .child(
        h_flex()
          .w_full()
          .gap_2()
          .flex_wrap()
          .child(filters)
          .child(div().flex_1())
          .child(div().text_xs().text_color(theme.muted_foreground).child(tf("catalog.visible_count", &[("count", &tools.len().to_string())]))),
      );

    if !cfg!(target_os = "macos") {
      body = body.child(Alert::info("platform-preview", t("platform.preview")));
    }
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
      let mut cards = v_flex().gap_2();
      for tool in &members {
        cards = cards.child(self.catalog_card(tool, cx));
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
              .child(div().text_size(px(11.)).text_color(theme.muted_foreground).truncate().child(description)),
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
    let version = match model.update_status(tool) {
      Some(UpdateStatus::Available { installed, latest }) => format!("v{installed} → v{latest}"),
      _ => tool.installed_version().map(|version| format!("v{version}")).unwrap_or_default(),
    };
    let has_update = model.can_update(tool);
    let can_update = has_update && !model.busy();

    h_flex()
      .id(SharedString::from(format!("catalog-{id}")))
      .w_full()
      .min_w_0()
      .gap_3()
      .p_3()
      .rounded(px(10.))
      .border_1()
      .border_color(if selected {
        theme.blue.opacity(0.65)
      } else if has_update {
        orange().opacity(0.5)
      } else {
        theme.border
      })
      .bg(if selected { theme.blue.opacity(0.06) } else { theme.background })
      .hover(move |style| style.bg(tint.opacity(0.04)).border_color(tint.opacity(0.4)))
      .child(selection_checkbox(&self.model, tool, false, cx))
      .child(tool_icon(tool, 32., cx))
      .child(
        v_flex()
          .flex_1()
          .min_w_0()
          .gap_1()
          .child(div().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child(tool.name))
          .child(div().text_xs().text_color(theme.muted_foreground).truncate().child(tool.subtitle())),
      )
      .child(
        v_flex()
          .gap_1()
          .items_end()
          .min_w(px(120.))
          .child(tag)
          .child(div().text_xs().text_color(if has_update { orange() } else { theme.muted_foreground }).child(version)),
      )
      .child(
        h_flex()
          .gap_2()
          .flex_none()
          .when(can_update, |row| {
            let update_model = self.model.clone();
            row.child(
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
              .small()
              .label(t("action.details"))
              .icon(IconName::ChevronRight)
              .accessibility_label(tf("action.details_accessibility", &[("name", tool.name)]))
              .on_click(cx.listener(move |this, _, _, cx| this.focus_tool(id, cx))),
          ),
      )
  }

  /// A clickable summary figure that filters the catalog.
  fn stat_tile(&self, label_key: &'static str, value: String, color: Hsla, filter: CatalogFilter, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let active = self.catalog_filter == filter;
    h_flex()
      .id(SharedString::from(format!("stat-{label_key}")))
      .flex_1()
      .gap_3()
      .px_4()
      .py_3()
      .rounded(px(12.))
      .cursor_pointer()
      .border_1()
      .border_color(if active { color.opacity(0.55) } else { theme.border })
      .bg(if active { color.opacity(0.07) } else { theme.foreground.opacity(0.025) })
      .hover(move |style| style.bg(color.opacity(0.08)))
      .on_click(cx.listener(move |this, _, _, cx| {
        if this.destination == Destination::Installed && filter == CatalogFilter::NotInstalled {
          this.destination = Destination::AllTools;
        }
        this.catalog_filter = if this.catalog_filter == filter { CatalogFilter::All } else { filter };
        cx.notify();
      }))
      .child(div().text_size(px(20.)).font_weight(FontWeight::SEMIBOLD).text_color(color).child(value))
      .child(caption(t(label_key), cx))
  }
}
