use super::{
  components::{green, orange},
  Destination, Workspace,
};
use crate::platform;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    h_flex,
    sidebar::{Sidebar, SidebarMenu, SidebarMenuItem},
    tag::Tag,
    v_flex, ActiveTheme, Icon, IconName, Sizable, TitleBar,
  },
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
    let items: Vec<_> = Destination::ALL
      .into_iter()
      .map(|destination| {
        let count = model.tools.iter().filter(|t| destination.includes(t, model)).count();
        let icon = match destination {
          Destination::AllTools => Icon::new(Lucide::Boxes),
          Destination::Installed => Icon::new(IconName::CircleCheck),
          Destination::Updates => Icon::new(Lucide::CircleArrowDown),
          Destination::Doctor => Icon::new(Lucide::Stethoscope),
          Destination::Settings => Icon::new(IconName::Settings),
        };
        SidebarMenuItem::new(destination.title())
          .icon(icon)
          .active(self.destination == destination)
          .on_click(cx.listener(move |this, _, _, cx| this.navigate(destination, cx)))
          .when(!matches!(destination, Destination::Settings | Destination::Doctor), |item| {
            item.suffix(move |_, _| {
              let tag = if destination == Destination::Updates && count > 0 { Tag::warning() } else { Tag::secondary() };
              tag.small().rounded_full().child(count.to_string())
            })
          })
      })
      .collect();

    Sidebar::<SidebarMenu>::new("sidebar")
      .w(px(224.))
      .collapsible(false)
      // The traffic lights sit in this band; it also drags the window.
      .header(TitleBar::new().h(super::TOOLBAR_HEIGHT).bg(transparent_black()).border_b_0())
      .child(SidebarMenu::new().children(items))
      .footer(
        v_flex().w_full().gap_2().child(
          h_flex()
            .id("channel")
            .gap(px(6.))
            .text_size(px(11.))
            .text_color(theme.muted_foreground)
            .child(div().size(px(5.)).rounded_full().bg(if stable { green() } else { orange() }))
            .child(format!("{} · v{APP_VERSION}", platform::channel_label()))
            .tooltip(|window, cx| {
              component::tooltip::Tooltip::new(tf("channel.tooltip", &[("channel", &platform::channel_label())])).build(window, cx)
            }),
        ),
      )
  }
}
