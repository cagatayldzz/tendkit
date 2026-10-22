use super::{
  components::{green, mono, orange},
  Workspace,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants},
    h_flex,
    spinner::Spinner,
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
    let summary = tf(
      "doctor.summary",
      &[
        ("warnings", &warning.to_string()),
        ("notices", &report.count(DoctorSeverity::Info).to_string()),
        ("passed", &report.count(DoctorSeverity::Passed).to_string()),
        ("unavailable", &report.count(DoctorSeverity::Unavailable).to_string()),
      ],
    );
    let mut body = v_flex()
      .w_full()
      .max_w(px(960.))
      .p_6()
      .gap_4()
      .child(
        v_flex()
          .gap_3()
          .p_4()
          .rounded(px(12.))
          .border_1()
          .border_color(theme.border)
          .child(
            h_flex()
              .flex_wrap()
              .gap_3()
              .child(
                v_flex()
                  .flex_1()
                  .min_w_0()
                  .gap_1()
                  .child(div().text_size(px(18.)).font_weight(FontWeight::SEMIBOLD).child(t("doctor.title")))
                  .child(div().text_sm().text_color(theme.muted_foreground).child(t("doctor.read_only"))),
              )
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
          .child(h_flex().gap_2().when(scanning, |row| row.child(Spinner::new().small())).child(
            div().text_sm().text_color(if warning > 0 && complete { orange() } else { theme.muted_foreground }).child(if scanning {
              t("doctor.scanning")
            } else if complete {
              summary
            } else {
              t("doctor.incomplete")
            }),
          ))
          .when_some(model.last_scan, |card, at| {
            card.child(
              div().text_xs().text_color(theme.muted_foreground).child(tf("doctor.checked_at", &[("time", &crate::platform::local_date_time(at))])),
            )
          }),
      )
      .child(div().text_sm().text_color(theme.muted_foreground).child(t("doctor.scope")));
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
    for check in checks {
      body = body.child(self.doctor_card(check, cx));
    }
    div().id("doctor-scroll").size_full().overflow_y_scroll().child(h_flex().w_full().justify_center().child(body))
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
    let mut card = v_flex().gap_3().p_4().rounded(px(12.)).border_1().border_color(theme.border).child(
      h_flex()
        .gap_3()
        .child(Icon::new(icon).size(px(24.)).text_color(color))
        .child(
          v_flex()
            .flex_1()
            .min_w_0()
            .gap_1()
            .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(check.name))
            .child(div().text_sm().text_color(theme.muted_foreground).child(title)),
        )
        .child(Tag::secondary().small().child(h_flex().gap_1().text_color(color).child(Icon::new(icon).size(px(12.))).child(t(status))))
        .child(
          Button::new(SharedString::from(format!("doctor-details-{id}")))
            .ghost()
            .small()
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
