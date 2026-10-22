use super::{
  components::{caption, card, mono, orange, selection_checkbox, status_tag, tool_icon},
  Workspace,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    alert::Alert,
    button::{Button, ButtonVariants},
    h_flex,
    spinner::Spinner,
    v_flex, ActiveTheme, Disableable, Icon, IconName, Sizable,
  },
  *,
};
use gpui_kit_assets::IconName as Lucide;
use std::collections::BTreeSet;
use tendkit_i18n::{t, tf};
use tendkit_model::{ToolRow, UpdateStatus};

impl Workspace {
  pub(super) fn render_detail(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let model = self.model.read(cx);
    let focused = self.focused_tool.and_then(|id| self.visible_tools(cx).into_iter().find(|t| t.id == id));
    let show_progress = model.is_executing || model.run_status.is_some();
    let centered = focused.is_none() && model.error.is_none() && !show_progress;
    let show_bar = !model.selected.is_empty() || model.is_planning;
    let error = model.error.clone();

    let mut body = v_flex().w_full().max_w(px(760.)).gap(px(24.)).p(px(30.));
    if let Some(error) = error {
      body = body.child(Alert::error("detail-error", error));
    }
    body = match &focused {
      Some(tool) => body.child(self.tool_details(tool, window, cx)),
      None => body.child(
        v_flex()
          .w_full()
          .items_center()
          .gap(px(14.))
          .child(Icon::new(Lucide::Boxes).size(px(44.)).text_color(theme.muted_foreground.opacity(0.6)))
          .child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).text_color(theme.muted_foreground).child(t("detail.empty_title")))
          .child(div().max_w(px(300.)).text_center().text_size(px(13.)).text_color(theme.muted_foreground).child(t("detail.empty_hint"))),
      ),
    };
    if show_progress {
      body = body.child(self.render_progress(cx));
    }

    v_flex()
      .size_full()
      .bg(theme.background)
      .child(
        div()
          .id("detail")
          .flex_1()
          .min_h_0()
          .overflow_y_scroll()
          .child(h_flex().w_full().min_h_full().justify_center().when(centered, |this| this.items_center()).child(body)),
      )
      .when(show_bar, |this| this.child(self.installation_bar(cx)))
  }

  fn tool_details(&self, tool: &ToolRow, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let latest = self.latest_version(tool, cx);
    let model = self.model.read(cx);
    let divider = || div().ml_4().h(px(1.)).bg(theme.border);
    let mut fields = card(cx)
      .child(detail_field(t("detail.status"), self.status(tool, cx), cx))
      .child(divider())
      .child(detail_field(t("detail.current"), mono(tool.installed_version().unwrap_or("—").to_string()), cx))
      .child(divider())
      .child(detail_field(t("detail.latest"), latest, cx));
    if let Some(installed) = tool.primary() {
      fields = fields.child(divider()).child(detail_field(t("detail.source"), div().child(installed.source.clone()), cx));
    }

    let locations = (!tool.installations.is_empty()).then(|| {
      v_flex().gap_3().child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).child(t("detail.locations"))).children(
        tool.installations.iter().map(|installed| {
          card(cx).w_full().p(px(14.)).child(
            v_flex()
              .gap(px(7.))
              .child(
                h_flex()
                  .gap_2()
                  .text_size(px(13.))
                  .font_weight(FontWeight::MEDIUM)
                  .child(Icon::new(IconName::Folder).size(px(13.)))
                  .child(installed.source.clone()),
              )
              .child(mono(installed.path.clone()).text_size(px(11.)).text_color(theme.muted_foreground))
              .when_some(installed.error.clone(), |this, error| this.child(div().text_size(px(11.)).text_color(orange()).child(error))),
          )
        }),
      )
    });

    let id = tool.id;
    let update_callout = match model.update_status(tool) {
      Some(UpdateStatus::Available { installed, latest }) => {
        let update_model = self.model.clone();
        let updatable = id != "npm" && model.can_manage(tool);
        Some(
          h_flex()
            .gap_3()
            .p(px(14.))
            .rounded(px(12.))
            .bg(orange().opacity(0.08))
            .border_1()
            .border_color(orange().opacity(0.35))
            .child(Icon::new(IconName::ArrowUp).size(px(16.)).text_color(orange()))
            .child(
              v_flex()
                .flex_1()
                .gap(px(3.))
                .child(div().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child(t("detail.update_available")))
                .child(mono(format!("{installed} → {latest}")).text_size(px(12.)).text_color(theme.muted_foreground))
                .when(id == "npm", |this| this.child(caption(t("detail.npm_note"), cx))),
            )
            .when(updatable, |this| {
              this.child(
                Button::new("update-now")
                  .primary()
                  .label(t("action.update"))
                  .disabled(model.busy())
                  .on_click(move |_, _, cx| update_model.update(cx, |model, cx| model.prepare(Some(BTreeSet::from([id.to_string()])), cx))),
              )
            }),
        )
      }
      _ => None,
    };
    let actions = if id == "npm" {
      h_flex()
        .gap_2()
        .text_size(px(13.))
        .text_color(theme.muted_foreground)
        .child(Icon::new(Lucide::Link).size(px(13.)))
        .child(t("detail.managed_with_node"))
        .into_any_element()
    } else {
      let label = match (id, tool.primary().is_some()) {
        ("homebrew", true) => t("action.refresh"),
        (_, false) => t("action.install"),
        _ => t("action.update"),
      };
      let prepare_model = self.model.clone();
      h_flex()
        .gap_2()
        .child(selection_checkbox(&self.model, tool, true, cx))
        .child(div().flex_1())
        .when(id == "opencommit" && tool.primary().is_some(), |this| {
          this.child(
            Button::new("opencommit-settings")
              .label(t("nav.settings"))
              .icon(Icon::new(Lucide::SlidersHorizontal))
              .tooltip(t("detail.opencommit_tooltip"))
              .disabled(model.is_executing)
              .on_click(cx.listener(|this, _, window, cx| this.open_opencommit_settings(window, cx))),
          )
        })
        .child(
          Button::new("prepare")
            .label(label)
            .disabled(model.busy() || !model.can_manage(tool))
            .on_click(move |_, _, cx| prepare_model.update(cx, |model, cx| model.prepare(Some(BTreeSet::from([id.to_string()])), cx))),
        )
        .into_any_element()
    };

    v_flex()
      .gap(px(24.))
      .child(
        h_flex().gap(px(15.)).child(tool_icon(tool, 54., cx)).child(
          v_flex()
            .gap(px(5.))
            .child(div().text_size(px(25.)).font_weight(FontWeight::SEMIBOLD).child(tool.name))
            .child(div().text_size(px(13.)).text_color(theme.muted_foreground).child(tool.subtitle())),
        ),
      )
      .children(update_callout)
      .child(fields)
      .children(locations)
      .child(actions)
  }

  fn status(&self, tool: &ToolRow, cx: &App) -> Div {
    div().child(status_tag(self.model.read(cx), tool))
  }

  fn latest_version(&self, tool: &ToolRow, cx: &mut Context<Self>) -> Div {
    let theme = cx.theme().clone();
    let model = self.model.read(cx);
    let id = tool.id;
    if model.checking_latest.contains(id) {
      h_flex().gap_2().child(Spinner::new().xsmall()).child(div().text_color(theme.muted_foreground).child(t("status.checking")))
    } else if model.latest_is_stale(id) {
      let error = model.latest_version_errors.get(id).cloned().unwrap_or_else(|| t("detail.cache_expired"));
      let retry_model = self.model.clone();
      let label = match (model.latest_versions.get(id), model.latest_checked_at(id)) {
        (Some(version), Some(at)) => tf("detail.stale", &[("version", version), ("time", &crate::platform::local_date_time(at))]),
        _ => t("detail.unavailable"),
      };
      h_flex().gap_2().child(div().text_color(theme.muted_foreground).child(label)).child(
        Button::new("retry-latest")
          .ghost()
          .xsmall()
          .icon(IconName::RefreshCw)
          .accessibility_label(t("detail.retry"))
          .tooltip(tf("detail.retry_tooltip", &[("error", &error)]))
          .on_click(move |_, _, cx| retry_model.update(cx, |model, cx| model.check_latest_version(id, true, cx))),
      )
    } else if let Some(version) = model.latest_versions.get(id) {
      let tip = if id == "npm" { t("detail.latest_npm_tip") } else { t("detail.latest_tip") };
      let newer = model.has_update(tool);
      div()
        .text_color(if newer { orange() } else { theme.foreground })
        .child(mono(version.clone()).id("latest-version").tooltip(move |window, cx| component::tooltip::Tooltip::new(tip.clone()).build(window, cx)))
    } else {
      div().text_color(theme.muted_foreground).child("—")
    }
  }

  pub(super) fn installation_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let model = self.model.read(cx);
    let prepare_model = self.model.clone();
    v_flex()
      .gap_3()
      .p(px(20.))
      .border_t_1()
      .border_color(theme.border)
      .child(
        h_flex()
          .gap_2()
          .when(model.is_planning, |this| this.child(Spinner::new().small()))
          .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).child(if model.is_planning {
            t("activity.planning")
          } else {
            tf("bar.selected", &[("count", &model.selected.len().to_string())])
          }))
          .child(div().flex_1())
          .child(
            Button::new("review-plan")
              .primary()
              .label(t("bar.review"))
              .icon(IconName::ArrowRight)
              .disabled(model.busy() || model.selected.is_empty())
              .on_click(move |_, _, cx| prepare_model.update(cx, |model, cx| model.prepare(None, cx))),
          ),
      )
      .child(caption(t("bar.note"), cx).text_color(theme.muted_foreground.opacity(0.7)))
  }
}

fn detail_field(title: String, content: impl IntoElement, cx: &App) -> impl IntoElement {
  h_flex().gap_4().p_4().text_size(px(13.)).child(div().text_color(cx.theme().muted_foreground).child(title)).child(div().flex_1()).child(content)
}
