use super::{
  components::{green, mono, orange},
  Workspace,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants},
    h_flex,
    progress::Progress,
    tag::Tag,
    v_flex, ActiveTheme, Disableable, Icon, IconName, Sizable,
  },
  *,
};
use gpui_kit_assets::IconName as Lucide;
use tendkit_i18n::{t, tf};
use tendkit_model::{DoctorCheck, DoctorSeverity};

impl Workspace {
  pub(super) fn render_doctor(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let model = self.model.read(cx);
    let report = model.doctor_report();
    let theme = cx.theme().clone();
    let busy = model.busy();
    let scanning = model.is_scanning;
    let complete = model.scan_complete && !scanning && !model.is_executing;
    let warning = report.count(DoctorSeverity::Warning) + usize::from(report.relative_path_entries > 0);
    let mut cards = h_flex().w_full().gap_3();
    for (severity, label, color) in [
      (DoctorSeverity::Passed, "doctor.passed", green()),
      (DoctorSeverity::Warning, "doctor.warning", orange()),
      (DoctorSeverity::Info, "doctor.notice", theme.blue),
      (DoctorSeverity::Unavailable, "doctor.unavailable", theme.muted_foreground),
    ] {
      let count = if severity == DoctorSeverity::Warning { warning } else { report.count(severity) };
      let active = self.doctor_filter == Some(severity);
      cards = cards.child(
        v_flex()
          .id(SharedString::from(format!("environment-stat-{label}")))
          .flex_1()
          .min_w_0()
          .p_4()
          .gap_2()
          .rounded(px(12.))
          .border_1()
          .border_color(if active { color } else { theme.border })
          .when(active, |card| card.bg(color.opacity(0.06)))
          .cursor_pointer()
          .hover(|style| style.bg(color.opacity(0.04)))
          .on_click(cx.listener(move |this, _, _, cx| {
            this.doctor_filter = if this.doctor_filter == Some(severity) { None } else { Some(severity) };
            cx.notify();
          }))
          .child(div().text_xs().text_color(theme.muted_foreground).child(t(label)))
          .child(
            div()
              .text_size(px(24.))
              .font_weight(FontWeight::SEMIBOLD)
              .text_color(if complete { color } else { theme.muted_foreground })
              .child(if complete { count.to_string() } else { "—".into() }),
          ),
      );
    }
    let header = v_flex()
      .w_full()
      .p_5()
      .gap_4()
      .border_b_1()
      .border_color(theme.border)
      .child(cards)
      .when(scanning, |header| {
        header.child(Progress::new("environment-scan-progress").small().loading(true).accessibility_label(t("doctor.scanning")))
      })
      .child(
        h_flex()
          .w_full()
          .gap_3()
          .flex_wrap()
          .child(
            v_flex()
              .flex_1()
              .min_w_0()
              .gap_1()
              .child(
                h_flex()
                  .gap_2()
                  .text_sm()
                  .child(
                    Icon::new(if complete && warning == 0 { Lucide::CircleCheck } else { Lucide::Terminal })
                      .size(px(16.))
                      .text_color(if complete && warning == 0 { green() } else { theme.muted_foreground }),
                  )
                  .child(if scanning {
                    t("doctor.scanning")
                  } else if complete {
                    t("doctor.scan_complete")
                  } else {
                    t("doctor.incomplete")
                  }),
              )
              .when_some(model.last_scan, |row, at| {
                row.child(
                  div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(tf("doctor.checked_at", &[("time", &crate::platform::local_date_time(at))])),
                )
              }),
          )
          .child(Button::new("doctor-scope-toggle").ghost().small().label(t("doctor.scope_details")).on_click(cx.listener(|this, _, _, cx| {
            this.doctor_scope_expanded = !this.doctor_scope_expanded;
            cx.notify();
          })))
          .child(
            Button::new("doctor-scan")
              .primary()
              .small()
              .icon(IconName::RefreshCw)
              .label(t("doctor.run"))
              .disabled(busy)
              .on_click(cx.listener(|this, _, _, cx| this.model.update(cx, |model, cx| model.scan_doctor(cx)))),
          ),
      )
      .when(self.doctor_scope_expanded, |header| {
        header.child(
          v_flex()
            .p_3()
            .gap_2()
            .rounded(px(8.))
            .bg(theme.foreground.opacity(0.025))
            .child(div().text_xs().child(t("doctor.read_only")))
            .child(div().text_xs().text_color(theme.muted_foreground).child(t("doctor.scope"))),
        )
      })
      .child(
        h_flex()
          .gap_2()
          .child(
            Button::new("environment-all")
              .ghost()
              .small()
              .label(t("doctor.all_checks"))
              .when(self.doctor_filter.is_none(), |button| button.bg(theme.blue.opacity(0.1)).text_color(theme.blue))
              .on_click(cx.listener(|this, _, _, cx| {
                this.doctor_filter = None;
                cx.notify();
              })),
          )
          .child(div().flex_1())
          .child(div().text_xs().text_color(theme.muted_foreground).child(t("doctor.priority_order"))),
      );
    let mut body = v_flex().w_full().p_5().gap_2();
    if let Some(error) = model.error.as_ref().filter(|_| !complete && !scanning) {
      body = body.child(component::alert::Alert::error("doctor-scan-error", error.clone()).title(t("doctor.scan_failed")));
    }
    if report.relative_path_entries > 0 {
      body = body.child(component::alert::Alert::warning(
        "doctor-relative-path",
        tf("doctor.relative_path", &[("count", &report.relative_path_entries.to_string())]),
      ));
    }
    let mut checks: Vec<_> = report.checks.iter().collect();
    checks.sort_by_key(|check| match check.severity {
      DoctorSeverity::Warning => 0,
      DoctorSeverity::Info => 1,
      DoctorSeverity::Passed => 2,
      DoctorSeverity::Unavailable => 3,
    });
    let mut shown = 0;
    for check in checks {
      if self.doctor_filter.is_none_or(|severity| check.severity == severity) {
        body = body.child(self.doctor_card(check, cx));
        shown += 1;
      }
    }
    if shown == 0 {
      body = body.child(div().p_6().text_center().text_sm().text_color(theme.muted_foreground).child(t("doctor.no_checks")));
    }
    v_flex().size_full().min_w_0().child(header).child(div().id("doctor-scroll").flex_1().min_h_0().w_full().overflow_y_scroll().child(body))
  }

  fn doctor_card(&self, check: &DoctorCheck, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let id = check.tool_id;
    let expanded = self.doctor_expanded.contains(id);
    let (icon, color, status) = match check.severity {
      DoctorSeverity::Passed => (Lucide::CircleCheck, green(), "doctor.passed"),
      DoctorSeverity::Info => (Lucide::Info, theme.blue, "doctor.notice"),
      DoctorSeverity::Warning => (Lucide::TriangleAlert, orange(), "doctor.warning"),
      DoctorSeverity::Unavailable => (Lucide::CircleMinus, theme.muted_foreground, "doctor.unavailable"),
    };
    let title = check.findings.first().map(|finding| t(&format!("doctor.{}.title", finding.key()))).unwrap_or_else(|| t("doctor.healthy"));
    let mut card = v_flex().gap_3().p_3().rounded(px(10.)).border_1().border_color(theme.border).child(
      h_flex()
        .gap_3()
        .child(div().p_2().rounded(px(8.)).bg(color.opacity(0.08)).child(Icon::new(icon).size(px(18.)).text_color(color)))
        .child(v_flex().flex_1().min_w_0().gap_1().child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(check.name)).child(
          div().text_xs().text_color(theme.muted_foreground).truncate().child(if check.severity == DoctorSeverity::Passed {
            check
              .installations
              .iter()
              .find(|entry| entry.active)
              .map(|entry| format!("{} · {}", entry.installation.version.as_deref().unwrap_or("—"), entry.installation.source))
              .unwrap_or(title)
          } else {
            title
          }),
        ))
        .child(Tag::secondary().small().child(h_flex().gap_1().text_color(color).child(Icon::new(icon).size(px(12.))).child(t(status))))
        .child(
          Button::new(SharedString::from(format!("doctor-details-{id}")))
            .ghost()
            .small()
            .icon(if expanded { IconName::ChevronDown } else { IconName::ChevronRight })
            .label(t(if expanded { "doctor.hide_details" } else { "doctor.details" }))
            .on_click(cx.listener(move |this, _, _, cx| {
              if !this.doctor_expanded.remove(id) {
                this.doctor_expanded.insert(id);
              }
              cx.notify();
            })),
        ),
    );
    if expanded {
      for finding in &check.findings {
        card = card.child(
          v_flex()
            .gap_1()
            .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(t(&format!("doctor.{}.title", finding.key()))))
            .child(div().text_sm().text_color(theme.muted_foreground).child(t(&format!("doctor.{}.advice", finding.key())))),
        );
      }
      for (index, entry) in check.installations.iter().enumerate() {
        let installation = &entry.installation;
        let position = if entry.active {
          t("doctor.active")
        } else if let Some(rank) = entry.path_rank {
          tf("doctor.path_position", &[("position", &(rank + 1).to_string())])
        } else {
          t("doctor.discovery_only")
        };
        card = card.child(
          v_flex()
            .min_w_0()
            .gap_1()
            .p_3()
            .rounded(px(8.))
            .bg(theme.foreground.opacity(0.035))
            .child(div().text_xs().text_color(theme.muted_foreground).child(position))
            .child(
              div()
                .id(SharedString::from(format!("doctor-path-{id}-{index}")))
                .w_full()
                .overflow_x_scroll()
                .child(mono(installation.path.clone()).text_size(px(12.)).whitespace_nowrap()),
            )
            .child(div().text_sm().child(format!(
              "{} · {}",
              installation.version.clone().unwrap_or_else(|| t("doctor.version_unknown")),
              installation.source
            )))
            .when_some(installation.error.clone(), |row, error| row.child(div().text_xs().text_color(orange()).child(error))),
        );
      }
      let command = check.command().to_owned();
      card = card.child(
        v_flex()
          .gap_2()
          .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(t("doctor.command")))
          .child(mono(command.clone()).text_size(px(12.)))
          .child(
            Button::new(SharedString::from(format!("doctor-copy-{id}")))
              .ghost()
              .small()
              .icon(Lucide::Copy)
              .label(t("doctor.copy"))
              .on_click(move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(command.clone()))),
          ),
      );
    }
    card
  }
}
