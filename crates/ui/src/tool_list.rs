use super::{
  components::{green, orange, selection_checkbox, tool_icon},
  Destination, Workspace,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants},
    h_flex,
    tag::Tag,
    v_flex, ActiveTheme, Disableable, Icon, IconName, Sizable,
  },
  *,
};
use gpui_kit_assets::IconName as Lucide;
use tendkit_i18n::{t, tf};
use tendkit_model::{ToolCategory, UpdateStatus};

impl Workspace {
  pub(super) fn render_tool_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let tools = self.visible_tools(cx);
    let searching = !self.query(cx).is_empty();
    let model = self.model.read(cx);
    let (scanning, scanned, installed, total) = (model.is_scanning, model.scanned_count(), model.installed_count(), model.tools.len());
    let updating = self.destination == Destination::Updates;
    let updatable = model.updatable_ids().len();
    let busy = model.busy();
    // One line per tool: the version, or `installed → latest` when an update exists.
    let lines: std::collections::HashMap<&str, (String, Hsla, bool)> = tools
      .iter()
      .map(|tool| {
        let line = match model.update_status(tool) {
          Some(UpdateStatus::Available { installed, latest }) => (format!("{installed} → {latest}"), orange(), model.can_update(tool)),
          Some(UpdateStatus::UpToDate) => (tool.installed_version().unwrap_or_default().to_string(), green(), false),
          _ if tool.scanned => {
            (tool.installed_version().map(str::to_string).unwrap_or_else(|| t("status.not_installed")), theme.muted_foreground, false)
          }
          _ => (t("status.waiting"), theme.muted_foreground, false),
        };
        (tool.id, line)
      })
      .collect();

    let list = if tools.is_empty() {
      v_flex()
        .flex_1()
        .items_center()
        .justify_center()
        .gap_2()
        .px_6()
        .child(
          if searching {
            Icon::new(IconName::Search)
          } else if updating {
            Icon::new(IconName::CircleCheck)
          } else {
            Icon::new(Lucide::Boxes)
          }
          .size(px(32.))
          .text_color(if updating && !searching { green() } else { theme.muted_foreground }),
        )
        .child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).child(if searching {
          t("list.no_results")
        } else if updating {
          t("list.up_to_date_title")
        } else {
          t("list.no_tools")
        }))
        .child(div().text_size(px(12.)).text_center().text_color(theme.muted_foreground).child(if searching {
          t("list.no_results_hint")
        } else if updating {
          t("list.up_to_date_hint")
        } else {
          t("list.empty_hint")
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
          let (status, status_color, has_update) = lines.get(id).cloned().unwrap_or_default();
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
                    .child(div().text_size(px(11.)).text_color(status_color).truncate().child(status)),
                )
                .when(has_update, |this| this.child(Tag::warning().small().child(t("action.update"))))
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
        .child(if scanning {
          tf("list.scanned", &[("scanned", &scanned.to_string()), ("total", &total.to_string())])
        } else if updating {
          tf("list.to_update", &[("count", &updatable.to_string())])
        } else {
          tf("statusbar.installed_count", &[("installed", &installed.to_string()), ("total", &total.to_string())])
        })
        .child(div().flex_1())
        .when(updatable > 0, |this| {
          this.child(
            Button::new("update-all-list")
              .primary()
              .xsmall()
              .label(t("action.update_all"))
              .disabled(busy)
              .on_click(cx.listener(|this, _, _, cx| this.model.update(cx, |model, cx| model.update_all(cx)))),
          )
        }),
    )
  }
}
