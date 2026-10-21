use super::{
  components::{selection_checkbox, tool_icon},
  Workspace,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{h_flex, v_flex, ActiveTheme, Icon, IconName},
  *,
};
use gpui_kit_assets::IconName as Lucide;
use tendkit_model::ToolCategory;

impl Workspace {
  pub(super) fn render_tool_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let tools = self.visible_tools(cx);
    let searching = !self.query(cx).is_empty();
    let model = self.model.read(cx);
    let (scanning, scanned, installed, total) = (model.is_scanning, model.scanned_count(), model.installed_count(), model.tools.len());

    let list = if tools.is_empty() {
      v_flex()
        .flex_1()
        .items_center()
        .justify_center()
        .gap_2()
        .px_6()
        .child(if searching { Icon::new(IconName::Search) } else { Icon::new(Lucide::Boxes) }.size(px(32.)).text_color(theme.muted_foreground))
        .child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).child(if searching { "No Results" } else { "No Tools" }))
        .child(div().text_size(px(12.)).text_center().text_color(theme.muted_foreground).child(if searching {
          "Try another tool name or installation source."
        } else {
          "Your developer tools will appear here."
        }))
        .into_any_element()
    } else {
      let mut rows = v_flex().id("tool-list").flex_1().min_h_0().overflow_y_scroll().py_2();
      for category in ToolCategory::ALL {
        let members: Vec<_> = tools.iter().filter(|t| t.category() == category).collect();
        if members.is_empty() {
          continue;
        }
        rows = rows.child(
          div().px_4().pt_3().pb_1().text_size(px(11.)).font_weight(FontWeight::SEMIBOLD).text_color(theme.muted_foreground).child(category.title()),
        );
        for tool in members {
          let focused = self.focused_tool == Some(tool.id);
          let id = tool.id;
          let status = if tool.scanned { tool.installed_version().unwrap_or("Not installed") } else { "Waiting for scan" };
          rows = rows.child(
            div().px_2().child(
              h_flex()
                .id(id)
                .gap(px(10.))
                .px_2()
                .py(px(7.))
                .rounded(px(10.))
                .cursor_pointer()
                .when(focused, |this| this.bg(theme.blue.opacity(0.14)))
                .when(!focused, |this| this.hover(|style| style.bg(theme.foreground.opacity(0.04))))
                .on_click(cx.listener(move |this, _, _, cx| this.focus_tool(id, cx)))
                .child(tool_icon(tool, 30., cx))
                .child(
                  v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).truncate().child(tool.name))
                    .child(div().text_size(px(11.)).text_color(theme.muted_foreground).truncate().child(status.to_string())),
                )
                .child(selection_checkbox(&self.model, tool, false, cx)),
            ),
          );
        }
      }
      rows.into_any_element()
    };

    v_flex().w(px(260.)).flex_none().h_full().bg(theme.sidebar.opacity(0.5)).border_r_1().border_color(theme.border).child(list).child(
      h_flex()
        .gap(px(6.))
        .p(px(14.))
        .border_t_1()
        .border_color(theme.border)
        .text_size(px(11.))
        .text_color(theme.muted_foreground)
        .child(if scanning { Icon::new(IconName::RefreshCw) } else { Icon::new(Lucide::ShieldCheck) }.size(px(12.)))
        .child(if scanning { format!("{scanned} of {total} scanned") } else { format!("{installed} of {total} installed") }),
    )
  }
}
