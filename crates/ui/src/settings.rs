use super::{components::caption, Workspace};
use crate::platform;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{button::Button, h_flex, switch::Switch, v_flex, ActiveTheme, Disableable, IconName},
  *,
};

/// A grouped settings section with an optional explanatory footer.
pub(super) fn section(title: &'static str, rows: Vec<AnyElement>, footer: Option<SharedString>, cx: &App) -> impl IntoElement {
  let theme = cx.theme();
  let divider = theme.border;
  let count = rows.len();
  v_flex()
    .gap(px(6.))
    .child(div().px_1().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).child(title))
    .child(
      v_flex().rounded(px(10.)).bg(theme.foreground.opacity(0.035)).border_1().border_color(theme.border.opacity(0.6)).children(
        rows
          .into_iter()
          .enumerate()
          .map(move |(index, row)| div().when(index + 1 < count, |this| this.border_b_1().border_color(divider)).px_3().py(px(9.)).child(row)),
      ),
    )
    .when_some(footer, |this, footer| this.child(caption(footer, cx).px_1()))
}

pub(super) fn labeled(label: impl Into<SharedString>, value: impl IntoElement, cx: &App) -> AnyElement {
  h_flex()
    .gap_4()
    .text_size(px(13.))
    .child(div().child(label.into()))
    .child(div().flex_1())
    .child(div().text_color(cx.theme().muted_foreground).child(value))
    .into_any_element()
}

impl Workspace {
  pub(super) fn render_settings(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let model = self.model.read(cx);
    let busy = model.busy();
    let switch_model = self.model.clone();
    let refresh_model = self.model.clone();

    let installation = vec![h_flex()
      .text_size(px(13.))
      .child("Configure terminal PATH")
      .child(div().flex_1())
      .child(Switch::new("configure-shell").checked(model.configure_shell).disabled(busy).on_click(move |checked, _, cx| {
        switch_model.update(cx, |model, cx| {
          model.configure_shell = *checked;
          cx.notify();
        })
      }))
      .into_any_element()];

    let versions = vec![
      labeled("Latest version cache", "6 hours", cx),
      h_flex()
        .child(
          Button::new("refresh-tools")
            .label("Refresh Tool Information")
            .icon(IconName::RefreshCw)
            .disabled(busy)
            .on_click(move |_, _, cx| refresh_model.update(cx, |model, cx| model.refresh(cx))),
        )
        .into_any_element(),
    ];

    let mut about = Vec::new();
    if !cfg!(debug_assertions) {
      about.push(labeled("Version", env!("CARGO_PKG_VERSION"), cx));
    }
    about.push(labeled("Channel", platform::channel(), cx));
    about.push(labeled("Architecture", platform::architecture(), cx));
    about.push(labeled(platform::os_name(), platform::os_version(), cx));
    if let Some(date) = model.last_scan {
      about.push(labeled("Last scan", platform::local_time(date), cx));
    }

    div().id("settings").size_full().overflow_y_scroll().child(
      v_flex()
        .w_full()
        .max_w(px(620.))
        .mx_auto()
        .gap(px(22.))
        .p(px(30.))
        .child(div().text_size(px(22.)).font_weight(FontWeight::SEMIBOLD).child("Settings"))
        .child(section(
          "Installation",
          installation,
          Some("Add managed tools to your terminal PATH when you approve an installation plan. Directories are appended, so existing PATH entries keep priority.".into()),
          cx,
        ))
        .child(section(
          "Version Checks",
          versions,
          Some("Refresh clears cached versions and scans installed tools. Latest versions are checked when you open a tool.".into()),
          cx,
        ))
        .child(section("About TendKit", about, None, cx)),
    )
  }
}
