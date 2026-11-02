use super::{
  components::{green, mono, orange},
  Workspace,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    alert::Alert,
    button::{Button, ButtonVariants},
    h_flex,
    notification::Notification,
    progress::Progress,
    tag::Tag,
    v_flex, ActiveTheme, Disableable, Icon, IconName, Sizable, WindowExt,
  },
  *,
};
use gpui_kit_assets::IconName as Lucide;
use std::path::Path;
use tendkit_i18n::{t, tf};
use tendkit_model::{DoctorCheck, DoctorReport, DoctorSeverity};

/// The colour, icon, and label key a severity is shown with.
fn severity_style(severity: DoctorSeverity, theme: &gpui_kit::component::Theme) -> (Lucide, Hsla, &'static str) {
  match severity {
    DoctorSeverity::Passed => (Lucide::CircleCheck, green(), "doctor.passed"),
    DoctorSeverity::Info => (Lucide::Info, theme.blue, "doctor.notice"),
    DoctorSeverity::Warning => (Lucide::TriangleAlert, orange(), "doctor.warning"),
    DoctorSeverity::Unavailable => (Lucide::CircleMinus, theme.muted_foreground, "doctor.unavailable"),
  }
}

impl Workspace {
  pub(super) fn render_doctor(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let model = self.model.read(cx);
    let report = model.doctor_report();
    let theme = cx.theme().clone();
    let busy = model.busy();
    let scanning = model.is_scanning;
    let complete = model.scan_complete && !scanning && !model.is_executing;
    let attention = report.attention_count();
    let notices = report.count(DoctorSeverity::Info);

    let (status_icon, status_color, status_title) = if !complete {
      (Lucide::Terminal, theme.muted_foreground, t("doctor.status_unknown"))
    } else if attention > 0 {
      (Lucide::TriangleAlert, orange(), tf("doctor.status_attention", &[("count", &attention.to_string())]))
    } else if notices > 0 {
      (Lucide::Info, theme.blue, tf("doctor.status_notices", &[("count", &notices.to_string())]))
    } else {
      (Lucide::CircleCheck, green(), t("doctor.status_healthy"))
    };
    let subtitle = if scanning {
      t("doctor.scanning")
    } else if let Some(at) = model.last_scan.filter(|_| complete) {
      tf("doctor.checked_at", &[("time", &crate::platform::local_date_time(at))])
    } else {
      t("doctor.incomplete")
    };

    let report_text = report.to_text();
    let top = h_flex()
      .w_full()
      .gap_3()
      .items_center()
      .child(
        div()
          .flex_none()
          .size(px(40.))
          .rounded_full()
          .bg(status_color.opacity(0.12))
          .flex()
          .items_center()
          .justify_center()
          .child(Icon::new(status_icon).size(px(20.)).text_color(status_color)),
      )
      .child(
        v_flex()
          .flex_1()
          .min_w_0()
          .gap(px(2.))
          .child(div().text_size(px(16.)).font_weight(FontWeight::SEMIBOLD).child(status_title))
          .child(div().text_xs().text_color(theme.muted_foreground).child(subtitle)),
      )
      .child(Button::new("doctor-copy-report").ghost().small().icon(Lucide::Copy).label(t("doctor.copy_report")).disabled(!complete).on_click(
        move |_, window, cx| {
          cx.write_to_clipboard(ClipboardItem::new_string(report_text.clone()));
          window.push_notification(Notification::success(t("notify.copied")), cx);
        },
      ))
      .child(
        Button::new("doctor-scan")
          .primary()
          .small()
          .icon(IconName::RefreshCw)
          .label(t("doctor.run"))
          .disabled(busy)
          .on_click(cx.listener(|this, _, _, cx| this.model.update(cx, |model, cx| model.scan_doctor(cx)))),
      );

    let mut filters = h_flex().gap_2().flex_wrap();
    let all = (None, t("doctor.filter_all"), report.checks.len(), theme.blue);
    let chips = std::iter::once(all).chain(
      [DoctorSeverity::Warning, DoctorSeverity::Info, DoctorSeverity::Passed, DoctorSeverity::Unavailable].into_iter().map(|severity| {
        let (_, color, label) = severity_style(severity, &theme);
        let count = if severity == DoctorSeverity::Warning { attention } else { report.count(severity) };
        (Some(severity), t(label), count, color)
      }),
    );
    for (severity, label, count, color) in chips {
      let active = self.doctor_filter == severity;
      filters = filters.child(
        Button::new(SharedString::from(format!("doctor-filter-{severity:?}")))
          .ghost()
          .small()
          .label(format!("{label} {}", if complete || severity.is_none() { count.to_string() } else { "—".into() }))
          .when(active, |button| button.bg(color.opacity(0.14)).text_color(color))
          .on_click(cx.listener(move |this, _, _, cx| {
            this.doctor_filter = if this.doctor_filter == severity { None } else { severity };
            cx.notify();
          })),
      );
    }

    let header = v_flex()
      .w_full()
      .p_5()
      .gap_4()
      .border_b_1()
      .border_color(theme.border)
      .child(top)
      .when(scanning, |header| {
        header.child(Progress::new("environment-scan-progress").small().loading(true).accessibility_label(t("doctor.scanning")))
      })
      .child(h_flex().w_full().gap_2().flex_wrap().child(filters).child(div().flex_1()).child(
        Button::new("doctor-scope-toggle").ghost().small().label(t("doctor.scope_details")).on_click(cx.listener(|this, _, _, cx| {
          this.doctor_scope_expanded = !this.doctor_scope_expanded;
          cx.notify();
        })),
      ))
      .when(self.doctor_scope_expanded, |header| {
        header.child(
          v_flex()
            .p_3()
            .gap_2()
            .rounded(px(8.))
            .bg(theme.group_box)
            .child(div().text_xs().child(t("doctor.read_only")))
            .child(div().text_xs().text_color(theme.muted_foreground).child(t("doctor.scope"))),
        )
      });

    let mut body = v_flex().w_full().p_5().gap_2();
    if let Some(error) = model.error.as_ref().filter(|_| !complete && !scanning) {
      body = body.child(Alert::error("doctor-scan-error", error.clone()).title(t("doctor.scan_failed")));
    }
    if report.relative_path_entries > 0 {
      body = body.child(Alert::warning("doctor-relative-path", tf("doctor.relative_path", &[("count", &report.relative_path_entries.to_string())])));
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
    if complete {
      body = body.child(self.doctor_path_section(&report, cx));
    }
    v_flex().size_full().min_w_0().child(header).child(div().id("doctor-scroll").flex_1().min_h_0().w_full().overflow_y_scroll().child(body))
  }

  /// The inherited PATH in lookup order, with the tools found in each directory.
  fn doctor_path_section(&self, report: &DoctorReport, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let expanded = self.doctor_path_expanded;
    let rows = report.path_rows();
    let path_text = report.path_entries.as_ref().map(|entries| entries.join(":"));
    let mut section = v_flex().gap_2().pt_4().child(
      h_flex()
        .gap_2()
        .child(div().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child(t("doctor.path_title")))
        .child(div().text_xs().text_color(theme.muted_foreground).child(rows.len().to_string()))
        .child(div().flex_1())
        .when_some(path_text, |row, text| {
          row.child(Button::new("doctor-copy-path-list").ghost().small().icon(Lucide::Copy).label(t("doctor.path_copy")).on_click(
            move |_, window, cx| {
              cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
              window.push_notification(Notification::success(t("notify.copied")), cx);
            },
          ))
        })
        .child(
          Button::new("doctor-path-toggle")
            .ghost()
            .small()
            .icon(if expanded { IconName::ChevronDown } else { IconName::ChevronRight })
            .label(t(if expanded { "doctor.hide_details" } else { "doctor.details" }))
            .on_click(cx.listener(|this, _, _, cx| {
              this.doctor_path_expanded = !this.doctor_path_expanded;
              cx.notify();
            })),
        ),
    );
    if report.path_entries.is_none() {
      return section.child(div().text_xs().text_color(theme.muted_foreground).child(t("doctor.path_unavailable")));
    }
    if !expanded {
      return section;
    }
    section = section.child(div().text_xs().text_color(theme.muted_foreground).child(t("doctor.path_hint")));
    let mut list = v_flex().rounded(px(10.)).bg(theme.group_box).border_1().border_color(theme.border);
    for (index, row) in rows.iter().enumerate() {
      // Only checked while the list is open, and only for absolute entries.
      let missing = !row.relative && !Path::new(&row.entry).is_dir();
      let flag = if row.relative {
        Some(t("doctor.path_relative"))
      } else if row.duplicate {
        Some(t("doctor.path_duplicate"))
      } else if missing {
        Some(t("doctor.path_missing"))
      } else {
        None
      };
      list = list.child(
        h_flex()
          .gap_3()
          .px_3()
          .py_2()
          .when(index > 0, |line| line.border_t_1().border_color(theme.border))
          .child(div().w(px(24.)).flex_none().text_xs().text_color(theme.muted_foreground).child((index + 1).to_string()))
          .child(div().flex_1().min_w_0().truncate().text_size(px(12.)).child(mono(row.entry.clone())))
          .when(!row.tools.is_empty(), |line| {
            line.child(h_flex().gap_1().flex_none().children(row.tools.iter().map(|name| Tag::secondary().small().child(*name))))
          })
          .when_some(flag, |line, flag| line.child(div().flex_none().text_xs().text_color(orange()).child(flag))),
      );
    }
    section.child(list)
  }

  fn doctor_card(&self, check: &DoctorCheck, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let id = check.tool_id;
    let expanded = self.doctor_expanded.contains(id);
    let (icon, color, status) = severity_style(check.severity, &theme);
    let title = check.findings.first().map(|finding| t(&format!("doctor.{}.title", finding.key()))).unwrap_or_else(|| t("doctor.healthy"));
    let shown = check.installations.iter().find(|entry| entry.active).or(check.installations.first());
    let summary = match shown {
      Some(entry) if check.severity == DoctorSeverity::Passed => {
        format!("{} · {}", entry.installation.version.as_deref().unwrap_or("—"), entry.installation.source)
      }
      Some(entry) => format!("{} · {}", title, entry.installation.source),
      None => title,
    };
    let path = shown.map(|entry| entry.installation.path.clone());

    let mut actions = h_flex().gap_1().flex_none();
    if let Some(path) = path.clone() {
      let copy = path.clone();
      let reveal = path.clone();
      actions = actions
        .child(
          Button::new(SharedString::from(format!("doctor-copy-path-{id}")))
            .ghost()
            .small()
            .icon(Lucide::Copy)
            .tooltip(t("disk.copy_path"))
            .accessibility_label(t("disk.copy_path"))
            .on_click(move |_, window, cx| {
              cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()));
              window.push_notification(Notification::success(t("notify.copied")), cx);
            }),
        )
        .child(
          Button::new(SharedString::from(format!("doctor-reveal-{id}")))
            .ghost()
            .small()
            .icon(Lucide::Folder)
            .tooltip(t(if cfg!(target_os = "macos") { "disk.reveal" } else { "disk.reveal_other" }))
            .accessibility_label(t("disk.reveal"))
            .on_click(move |_, _, cx| cx.reveal_path(Path::new(&reveal))),
        );
    }
    actions = actions.child(
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
    );

    let mut card = v_flex().gap_3().p_3().rounded(px(10.)).border_1().border_color(theme.border).bg(theme.group_box).child(
      h_flex()
        .gap_3()
        .child(div().flex_none().p_2().rounded(px(8.)).bg(color.opacity(0.1)).child(Icon::new(icon).size(px(18.)).text_color(color)))
        .child(
          v_flex()
            .w(px(220.))
            .flex_none()
            .gap_1()
            .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(check.name))
            .child(div().text_xs().text_color(theme.muted_foreground).truncate().child(summary)),
        )
        .child(
          div().flex_1().min_w_0().truncate().text_size(px(12.)).text_color(theme.muted_foreground).child(mono(path.clone().unwrap_or_default())),
        )
        .child(Tag::secondary().small().child(h_flex().gap_1().text_color(color).child(Icon::new(icon).size(px(12.))).child(t(status))))
        .child(actions),
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
            .bg(theme.background)
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
      let terminal_dir = path.as_deref().and_then(|path| Path::new(path).parent()).map(Path::to_path_buf);
      card = card.child(
        v_flex()
          .gap_2()
          .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(t("doctor.command")))
          .child(mono(command.clone()).text_size(px(12.)))
          .child(
            h_flex()
              .gap_2()
              .child(
                Button::new(SharedString::from(format!("doctor-copy-{id}")))
                  .ghost()
                  .small()
                  .icon(Lucide::Copy)
                  .label(t("doctor.copy"))
                  .on_click(move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(command.clone()))),
              )
              .when_some(terminal_dir, |row, dir| {
                row.child(
                  Button::new(SharedString::from(format!("doctor-terminal-{id}")))
                    .ghost()
                    .small()
                    .icon(Lucide::Terminal)
                    .label(t("disk.terminal"))
                    .disabled(!cfg!(target_os = "macos"))
                    .on_click(move |_, window, cx| {
                      if let Err(error) = crate::platform::open_terminal(&dir) {
                        window.push_notification(Notification::error(tf("disk.action_error", &[("error", &error.to_string())])), cx);
                      }
                    }),
                )
              }),
          ),
      );
    }
    card
  }
}
