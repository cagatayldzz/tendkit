use super::{components::green, Destination, Workspace};
use crate::platform;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{h_flex, v_flex, ActiveTheme, Icon, IconName, TitleBar},
  *,
};
use gpui_kit_assets::IconName as Lucide;

impl Workspace {
  pub(super) fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let stable = platform::channel() == "Stable";
    v_flex()
      .w(px(224.))
      .flex_none()
      .h_full()
      .bg(theme.sidebar)
      .border_r_1()
      .border_color(theme.sidebar_border)
      // The traffic lights sit in this band; it also drags the window.
      .child(TitleBar::new().h(super::TOOLBAR_HEIGHT).bg(transparent_black()).border_b_0())
      .child(h_flex().flex_wrap().gap_2().p_3().children(Destination::ALL.into_iter().map(|destination| self.navigation_card(destination, cx))))
      .child(div().flex_1())
      .child(
        h_flex()
          .id("channel")
          .gap(px(6.))
          .p_4()
          .text_size(px(11.))
          .text_color(theme.muted_foreground)
          .child(div().size(px(5.)).rounded_full().bg(if stable { green() } else { super::components::orange() }))
          .child(if stable { "Stable" } else { "DEV" })
          .tooltip(|window, cx| component::tooltip::Tooltip::new(format!("TendKit {}", platform::channel())).build(window, cx)),
      )
  }

  fn navigation_card(&self, destination: Destination, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let active = self.destination == destination;
    let count = self.model.read(cx).tools.iter().filter(|t| destination.includes(t)).count();
    let (icon, tint): (Icon, Hsla) = match destination {
      Destination::AllTools => (Icon::new(Lucide::Boxes), theme.blue),
      Destination::Installed => (Icon::new(IconName::CircleCheck), green()),
      Destination::Settings => (Icon::new(IconName::Settings), theme.muted_foreground),
    };
    let foreground = if active { white() } else { theme.foreground };
    v_flex()
      .id(destination.title())
      .w(px(94.))
      .gap(px(7.))
      .p(px(11.))
      .rounded(px(18.))
      .cursor_pointer()
      .bg(if active { theme.blue } else { theme.foreground.opacity(0.045) })
      .when(!active, |this| this.hover(|style| style.bg(theme.foreground.opacity(0.08))))
      .on_click(cx.listener(move |this, _, _, cx| this.navigate(destination, cx)))
      .child(
        h_flex()
          .child(
            div()
              .size(px(24.))
              .rounded_full()
              .flex()
              .items_center()
              .justify_center()
              .bg(if active { white().opacity(0.22) } else { tint })
              .child(icon.size(px(13.)).text_color(white())),
          )
          .child(div().flex_1())
          .when(destination != Destination::Settings, |this| {
            this.child(
              div()
                .text_size(px(10.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(if active { foreground } else { theme.muted_foreground })
                .child(count.to_string()),
            )
          }),
      )
      .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).text_color(foreground).truncate().child(destination.title()))
  }
}
