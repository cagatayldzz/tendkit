use super::{
  components::{caption, card, green, mono, orange, selection_checkbox, tool_icon},
  Workspace,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants},
    h_flex,
    spinner::Spinner,
    v_flex, ActiveTheme, Disableable, Icon, IconName, Sizable,
  },
  *,
};
use gpui_kit_assets::IconName as Lucide;
use std::collections::BTreeSet;
use tendkit_model::ToolRow;

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
      body = body.child(
        h_flex()
          .gap_2()
          .p(px(14.))
          .rounded(px(12.))
          .bg(orange().opacity(0.08))
          .text_size(px(13.))
          .text_color(orange())
          .child(Icon::new(IconName::TriangleAlert).size(px(14.)))
          .child(div().flex_1().child(error)),
      );
    }
    body = match &focused {
      Some(tool) => body.child(self.tool_details(tool, window, cx)),
      None => body.child(
        v_flex()
          .w_full()
          .items_center()
          .gap(px(14.))
          .child(Icon::new(Lucide::Boxes).size(px(44.)).text_color(theme.muted_foreground.opacity(0.6)))
          .child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).text_color(theme.muted_foreground).child("Your Developer Environment"))
          .child(
            div()
              .max_w(px(300.))
              .text_center()
              .text_size(px(13.))
              .text_color(theme.muted_foreground)
              .child("Select a tool to see its version, installation details, and available actions."),
          ),
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
      .child(detail_field("Status", self.status(tool, cx), cx))
      .child(divider())
      .child(detail_field("Current Version", mono(tool.installed_version().unwrap_or("—").to_string()), cx))
      .child(divider())
      .child(detail_field("Latest Version", latest, cx));
    if let Some(installed) = tool.primary() {
      fields = fields.child(divider()).child(detail_field("Source", div().child(installed.source.clone()), cx));
    }

    let locations = (!tool.installations.is_empty()).then(|| {
      v_flex().gap_3().child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).child("Installation Locations")).children(
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
    let actions = if id == "npm" {
      h_flex()
        .gap_2()
        .text_size(px(13.))
        .text_color(theme.muted_foreground)
        .child(Icon::new(Lucide::Link).size(px(13.)))
        .child("Managed with Node.js")
        .into_any_element()
    } else {
      let label = match (id, tool.primary().is_some()) {
        ("homebrew", true) => "Refresh",
        (_, false) => "Install",
        _ => "Update",
      };
      let prepare_model = self.model.clone();
      h_flex()
        .gap_2()
        .child(selection_checkbox(&self.model, tool, true, cx))
        .child(div().flex_1())
        .when(id == "opencommit" && tool.primary().is_some(), |this| {
          this.child(
            Button::new("opencommit-settings")
              .label("Settings")
              .icon(Icon::new(Lucide::SlidersHorizontal))
              .tooltip("Configure OpenCommit's model, language, and commit message style")
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
            .child(div().text_size(px(13.)).text_color(theme.muted_foreground).child(tool.subtitle)),
        ),
      )
      .child(fields)
      .children(locations)
      .child(actions)
  }

  fn status(&self, tool: &ToolRow, cx: &App) -> Div {
    let theme = cx.theme();
    let (icon, label, color): (Icon, &str, Hsla) = if !tool.scanned {
      (Icon::new(IconName::Ellipsis), if self.model.read(cx).is_scanning { "Scanning" } else { "Pending" }, theme.muted_foreground)
    } else if let Some(installation) = tool.primary() {
      if installation.version.is_some() {
        (Icon::new(IconName::CircleCheck), "Installed", green())
      } else {
        (Icon::new(IconName::CircleAlert), "Needs attention", orange())
      }
    } else {
      (Icon::new(Lucide::CircleArrowDown), "Not installed", theme.muted_foreground)
    };
    h_flex().gap(px(6.)).text_color(color).child(icon.size(px(13.))).child(label)
  }

  fn latest_version(&self, tool: &ToolRow, cx: &mut Context<Self>) -> Div {
    let theme = cx.theme().clone();
    let model = self.model.read(cx);
    let id = tool.id;
    if model.checking_latest.contains(id) {
      h_flex().gap_2().child(Spinner::new().xsmall()).child(div().text_color(theme.muted_foreground).child("Checking…"))
    } else if let Some(error) = model.latest_version_errors.get(id) {
      let retry_model = self.model.clone();
      h_flex().gap_2().child(div().text_color(theme.muted_foreground).child("Unavailable")).child(
        Button::new("retry-latest")
          .ghost()
          .xsmall()
          .icon(IconName::RefreshCw)
          .accessibility_label("Retry checking the latest version")
          .tooltip(format!("Retry checking the latest version. {error}"))
          .on_click(move |_, _, cx| retry_model.update(cx, |model, cx| model.check_latest_version(id, true, cx))),
      )
    } else if let Some(version) = model.latest_versions.get(id) {
      let tip = if id == "npm" {
        "Latest npm release. npm is installed and updated with Node.js."
      } else {
        "Latest version for this tool's installation source and release channel."
      };
      div().child(mono(version.clone()).id("latest-version").tooltip(move |window, cx| component::tooltip::Tooltip::new(tip).build(window, cx)))
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
            "Preparing plan…".to_string()
          } else {
            format!("{} tools selected", model.selected.len())
          }))
          .child(div().flex_1())
          .child(
            Button::new("review-plan")
              .primary()
              .label("Review Plan")
              .icon(IconName::ArrowRight)
              .disabled(model.busy() || model.selected.is_empty())
              .on_click(move |_, _, cx| prepare_model.update(cx, |model, cx| model.prepare(None, cx))),
          ),
      )
      .child(caption("Changes begin only after you approve the plan.", cx).text_color(theme.muted_foreground.opacity(0.7)))
  }
}

fn detail_field(title: &'static str, content: impl IntoElement, cx: &App) -> impl IntoElement {
  h_flex().gap_4().p_4().text_size(px(13.)).child(div().text_color(cx.theme().muted_foreground).child(title)).child(div().flex_1()).child(content)
}
