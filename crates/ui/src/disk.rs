use super::Workspace;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    alert::Alert,
    breadcrumb::{Breadcrumb, BreadcrumbItem},
    button::{Button, ButtonVariants},
    h_flex,
    input::Input,
    progress::Progress,
    skeleton::Skeleton,
    tag::Tag,
    v_flex, ActiveTheme, Disableable, Icon, IconName, Sizable,
  },
  *,
};
use gpui_kit_assets::IconName as Lucide;
use std::path::PathBuf;
use tendkit_i18n::{t, tf};

fn size(bytes: u64) -> String {
  let units = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
  let mut value = bytes as f64;
  let mut unit = 0;
  while value >= 1024. && unit < units.len() - 1 {
    value /= 1024.;
    unit += 1;
  }
  format!("{value:.1} {}", units[unit])
}

impl Workspace {
  fn browse_disk(&mut self, path: PathBuf, cx: &mut Context<Self>) {
    self.disk_visible = 100;
    self.model.update(cx, |model, cx| model.browse_disk(path, cx));
  }

  pub(super) fn render_disk(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let disk = &self.model.read(cx).disk;
    let theme = cx.theme().clone();
    let scanning = disk.scanning;
    let current = disk.path.clone();
    let parent = current.parent().map(|path| path.to_path_buf());
    let home = tendkit_storage::paths::home_dir();
    let capacity = disk.index.as_ref().and_then(|index| index.report(&index.root)).and_then(|report| report.capacity);
    let mut summary = h_flex().w_full().gap_4().flex_wrap();
    for (label, value) in [("disk.total", capacity.map(|v| v.0)), ("disk.used", capacity.map(|v| v.1)), ("disk.free", capacity.map(|v| v.2))] {
      summary = summary.child(
        h_flex()
          .gap_2()
          .child(div().text_xs().text_color(theme.muted_foreground).child(t(label)))
          .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(value.map(size).unwrap_or_else(|| "—".into()))),
      );
    }
    let mut header = v_flex().w_full().gap_2().px_5().py_3().border_b_1().border_color(theme.border).child(summary);
    let mut crumbs = Breadcrumb::new();
    let mut ancestors: Vec<_> = current.ancestors().take(4).map(|path| path.to_path_buf()).collect();
    ancestors.reverse();
    for path in ancestors {
      let label = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_else(|| t("disk.root"));
      crumbs = crumbs.child(BreadcrumbItem::new(label).on_click(cx.listener(move |this, _, _, cx| this.browse_disk(path.clone(), cx))));
    }
    header = header
      .child(
        h_flex()
          .w_full()
          .gap_2()
          .child(
            Button::new("disk-home")
              .ghost()
              .small()
              .icon(Lucide::House)
              .tooltip(t("disk.home"))
              .on_click(cx.listener(move |this, _, _, cx| this.browse_disk(home.clone(), cx))),
          )
          .child(Button::new("disk-up").ghost().small().icon(IconName::ArrowUp).tooltip(t("disk.up")).disabled(parent.is_none()).on_click(
            cx.listener(move |this, _, _, cx| {
              if let Some(path) = &parent {
                this.browse_disk(path.clone(), cx);
              }
            }),
          ))
          .child(div().flex_1().min_w_0().overflow_hidden().child(crumbs))
          .child(if scanning {
            Button::new("disk-stop")
              .ghost()
              .small()
              .label(t("disk.stop"))
              .on_click(cx.listener(|this, _, _, cx| this.model.read(cx).cancel_disk()))
              .into_any_element()
          } else {
            Button::new("disk-refresh")
              .ghost()
              .small()
              .icon(IconName::RefreshCw)
              .label(t("disk.refresh"))
              .on_click(cx.listener(move |this, _, _, cx| this.model.update(cx, |model, cx| model.refresh_disk_index(cx))))
              .into_any_element()
          }),
      )
      .child(
        h_flex()
          .w_full()
          .gap_2()
          .flex_wrap()
          .child(
            div().flex_1().min_w(px(140.)).child(Input::new(&self.disk_search).small().prefix(Icon::new(IconName::Search).small()).cleanable(true)),
          )
          .child(Button::new("disk-sort").ghost().small().label(t(if self.disk_sort_name { "disk.sort_name" } else { "disk.sort_size" })).on_click(
            cx.listener(|this, _, _, cx| {
              this.disk_sort_name = !this.disk_sort_name;
              this.disk_visible = 100;
              cx.notify();
            }),
          ))
          .child(
            Button::new("disk-hidden")
              .ghost()
              .small()
              .label(t(if self.disk_show_hidden { "disk.hide_hidden" } else { "disk.show_hidden" }))
              .on_click(cx.listener(|this, _, _, cx| {
                this.disk_show_hidden = !this.disk_show_hidden;
                this.disk_visible = 100;
                cx.notify();
              })),
          ),
      );
    header = header.when(scanning, |header| header.child(div().text_xs().text_color(theme.muted_foreground).child(t("disk.scanning"))));
    let mut list = v_flex().w_full().gap_0();
    if scanning && disk.report.is_none() {
      list = list.child(v_flex().p_5().gap_3().child(div().text_sm().child(t("disk.scanning"))).children((0..5).map(|_| Skeleton::new().h(px(42.)))));
    }
    if disk.error.is_some() {
      list = list.child(div().p_4().child(Alert::error("disk-error", t("disk.error"))));
    }
    if let Some(index) = &disk.index {
      list = list.child(div().px_5().pt_2().text_xs().text_color(theme.muted_foreground).child(tf(
        "disk.index_date",
        &[("time", &crate::platform::local_date_time(std::time::UNIX_EPOCH + std::time::Duration::from_secs(index.scanned_at)))],
      )));
    }
    if disk.cache_error {
      list = list.child(div().px_5().py_2().text_xs().child(t("disk.cache_error")));
    }
    if !scanning && disk.index.is_some() && disk.report.is_none() {
      list = list.child(div().p_5().text_sm().child(t("disk.not_indexed")));
    }
    if let Some(report) = &disk.report {
      let total = report.entries.iter().map(|entry| entry.bytes).fold(0, u64::saturating_add);
      let query = self.disk_search.read(cx).value().trim().to_lowercase();
      let mut entries: Vec<_> = report
        .entries
        .iter()
        .filter(|entry| {
          let name = entry.path.file_name().unwrap_or_default().to_string_lossy();
          (self.disk_show_hidden || !name.starts_with('.')) && name.to_lowercase().contains(&query)
        })
        .collect();
      if self.disk_sort_name {
        entries.sort_by_key(|entry| entry.path.file_name().unwrap_or_default().to_string_lossy().to_lowercase());
      }
      list = list.child(
        h_flex()
          .w_full()
          .px_5()
          .py_3()
          .gap_2()
          .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(tf("disk.items", &[("count", &entries.len().to_string())])))
          .child(div().flex_1())
          .child(Tag::secondary().small().child(tf("disk.measured", &[("size", &size(total))]))),
      );
      if report.partial || report.skipped > 0 {
        list = list.child(div().px_5().pb_2().text_xs().text_color(theme.muted_foreground).child(t("disk.index_partial")));
      }
      if report.excluded > 0 {
        list = list.child(
          div()
            .px_5()
            .pb_3()
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(tf("disk.excluded_count", &[("count", &report.excluded.to_string())])),
        );
      }
      list = list.child(
        h_flex()
          .px_5()
          .py_2()
          .gap_3()
          .bg(theme.foreground.opacity(0.025))
          .text_xs()
          .text_color(theme.muted_foreground)
          .child(div().flex_1().child(t("disk.name")))
          .child(div().w(px(140.)).child(t("disk.share")))
          .child(div().w(px(105.)).text_right().child(t("disk.size"))),
      );
      for (index, entry) in entries.iter().take(self.disk_visible).enumerate() {
        let path = entry.path.clone();
        let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let fraction = if total > 0 { entry.bytes as f32 / total as f32 * 100. } else { 0. };
        let clickable = entry.directory && !entry.excluded;
        list = list.child(
          h_flex()
            .id(SharedString::from(format!("disk-row-{index}")))
            .w_full()
            .min_h(px(58.))
            .px_5()
            .py_2()
            .gap_3()
            .border_b_1()
            .border_color(theme.border.opacity(0.5))
            .when(clickable, |row| {
              row
                .cursor_pointer()
                .hover(|style| style.bg(theme.blue.opacity(0.06)))
                .on_click(cx.listener(move |this, _, _, cx| this.browse_disk(path.clone(), cx)))
            })
            .child(
              Icon::new(if entry.excluded || !entry.measured {
                Lucide::ShieldCheck
              } else if entry.directory {
                Lucide::Folder
              } else {
                Lucide::File
              })
              .size(px(20.))
              .text_color(if entry.directory { theme.blue } else { theme.muted_foreground }),
            )
            .child(v_flex().flex_1().min_w_0().gap_1().child(div().text_sm().font_weight(FontWeight::MEDIUM).truncate().child(name)).when(
              entry.excluded || !entry.measured,
              |column| {
                column.child(div().text_xs().text_color(theme.muted_foreground).child(t(if entry.excluded {
                  "disk.excluded"
                } else {
                  "disk.unmeasured"
                })))
              },
            ))
            .child(
              v_flex()
                .w(px(140.))
                .gap_1()
                .child(div().text_xs().text_color(theme.muted_foreground).child(if entry.excluded || !entry.measured {
                  "—".into()
                } else {
                  format!("{fraction:.1}%")
                }))
                .child(Progress::new(SharedString::from(format!("disk-share-{index}"))).small().value(fraction).accessibility_label(t("disk.share"))),
            )
            .child(div().w(px(105.)).text_right().text_sm().child(if entry.excluded || !entry.measured { "—".into() } else { size(entry.bytes) })),
        );
      }
      if entries.len() > self.disk_visible {
        list =
          list.child(div().p_3().child(Button::new("disk-more").ghost().small().label(t("disk.more")).on_click(cx.listener(|this, _, _, cx| {
            this.disk_visible += 100;
            cx.notify();
          }))));
      }
      if entries.is_empty() {
        list = list.child(div().p_6().text_center().text_color(theme.muted_foreground).child(t("disk.empty")));
      }
    }
    div().id("disk-scroll").size_full().overflow_y_scroll().child(v_flex().w_full().min_w_0().child(header).child(list))
  }
}
