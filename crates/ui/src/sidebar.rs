use super::{
  components::{green, orange},
  Destination, Workspace,
};
use crate::platform;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{h_flex, v_flex, ActiveTheme, Icon, IconName, TitleBar},
  *,
};
use gpui_kit_assets::IconName as Lucide;
use tendkit_i18n::tf;
use tendkit_model::APP_VERSION;

impl Workspace {
  pub(super) fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let stable = platform::channel() == "Stable";
    let model = self.model.read(cx);
    let tool_count = model.tools.len();
    let items: Vec<AnyElement> = Destination::ALL
      .into_iter()
      .map(|destination| {
        let icon = match destination {
          Destination::Tools => Icon::new(Lucide::Boxes),
          Destination::Doctor => Icon::new(Lucide::Terminal),
          Destination::Disk => Icon::new(Lucide::HardDrive),
          Destination::Applications => Icon::new(Lucide::AppWindow),
          Destination::Settings => Icon::new(IconName::Settings),
        };
        let selected = self.destination == destination;
        let (selected_background, hover_background, foreground) = (theme.sidebar_accent, theme.list_hover, theme.foreground);
        h_flex()
          .id(SharedString::from(format!("sidebar-{}", destination.title())))
          .w_full()
          .h_9()
          .px_3()
          .gap_3()
          .items_center()
          .rounded_md()
          .cursor_pointer()
          .text_sm()
          .font_weight(if selected { FontWeight::MEDIUM } else { FontWeight::NORMAL })
          .text_color(if selected { theme.foreground } else { theme.muted_foreground })
          .when(selected, |row| row.bg(selected_background))
          .when(!selected, |row| row.hover(move |row| row.bg(hover_background).text_color(foreground)))
          .child(icon.size_4())
          .child(div().flex_1().child(destination.title()))
          .when(destination.is_tool_catalog(), |row| {
            row.child(div().px_2().rounded_full().bg(theme.border).text_color(theme.foreground).text_xs().child(tool_count.to_string()))
          })
          .on_click(cx.listener(move |this, _, _, cx| this.navigate(destination, cx)))
          .into_any_element()
      })
      .collect();

    v_flex()
      .id("sidebar")
      .w(px(224.))
      .h_full()
      .flex_none()
      .bg(theme.title_bar)
      .border_r_1()
      .border_color(theme.border)
      // The traffic lights sit in this band; it also drags the window.
      .child(TitleBar::new().h(super::TOOLBAR_HEIGHT).bg(transparent_black()).border_b_0())
      .child(v_flex().flex_1().min_h_0().px_2().pt_2().gap_0p5().children(items))
      .child(
        h_flex()
          .id("channel")
          .px_4()
          .py_3()
          .gap(px(6.))
          .items_center()
          .text_xs()
          .text_color(theme.muted_foreground)
          .child(div().size(px(5.)).rounded_full().bg(if stable { green() } else { orange() }))
          .child(format!("{} · v{APP_VERSION}", platform::channel_label()))
          .tooltip(|window, cx| {
            component::tooltip::Tooltip::new(tf("channel.tooltip", &[("channel", &platform::channel_label())])).build(window, cx)
          }),
      )
  }
}
