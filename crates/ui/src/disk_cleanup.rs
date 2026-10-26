use super::{disk::size, Workspace};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    h_flex,
    tag::Tag,
    v_flex, ActiveTheme, Disableable, Sizable, WindowExt,
  },
  *,
};
use tendkit_i18n::{t, tf};
use tendkit_model::AppModel;
use tendkit_storage::cleanup::{self, CleanupCandidate, CleanupKind};

struct CleanupRow {
  candidate: CleanupCandidate,
  selected: bool,
  moved: bool,
  error: Option<String>,
}

struct DiskCleanup {
  model: Entity<AppModel>,
  rows: Vec<CleanupRow>,
  loading: bool,
  busy: bool,
  completed: bool,
  error: Option<String>,
}

fn category(kind: CleanupKind) -> &'static str {
  match kind {
    CleanupKind::BunCache => "cleanup.bun",
    CleanupKind::RustTarget => "cleanup.rust",
    CleanupKind::NodeModules => "cleanup.node",
    CleanupKind::PythonEnvironment => "cleanup.python",
  }
}

impl DiskCleanup {
  fn new(proposals: Vec<CleanupCandidate>, model: Entity<AppModel>, cx: &mut Context<Self>) -> Self {
    let task = cx.background_executor().spawn(async move { cleanup::prepare(proposals, &tendkit_storage::paths::home_dir()) });
    cx.spawn(async move |this, cx| {
      let candidates = task.await;
      let _ = this.update(cx, |this, cx| {
        this.loading = false;
        this.rows = candidates
          .into_iter()
          .map(|candidate| CleanupRow { selected: candidate.kind.recommended(), candidate, moved: false, error: None })
          .collect();
        cx.notify();
      });
    })
    .detach();
    Self { model, rows: Vec::new(), loading: true, busy: false, completed: false, error: None }
  }

  fn move_selected(&mut self, cx: &mut Context<Self>) {
    if self.busy || self.loading {
      return;
    }
    let selected: Vec<_> =
      self.rows.iter().enumerate().filter(|(_, row)| row.selected && !row.moved).map(|(index, row)| (index, row.candidate.clone())).collect();
    if selected.is_empty() {
      return;
    }
    self.busy = true;
    self.error = None;
    let task = cx.background_executor().spawn(async move {
      cleanup::invalidate_cache()?;
      let home = tendkit_storage::paths::home_dir();
      let results: Vec<_> = selected
        .into_iter()
        .map(|(index, candidate)| {
          let result = cleanup::move_to_trash(&candidate, &home, crate::platform::trash_directory);
          (index, result)
        })
        .collect();
      Ok::<_, String>(results)
    });
    cx.spawn(async move |this, cx| {
      let result = task.await;
      let _ = this.update(cx, |this, cx| {
        this.busy = false;
        match result {
          Ok(results) => {
            for (index, result) in results {
              let row = &mut this.rows[index];
              match result {
                Ok(()) => {
                  row.moved = true;
                  row.selected = false;
                  row.error = None;
                }
                Err(error) => row.error = Some(error),
              }
            }
            this.completed = true;
            this.model.update(cx, |model, cx| model.invalidate_disk_index(cx));
          }
          Err(error) => this.error = Some(error),
        }
        cx.notify();
      });
    })
    .detach();
    cx.notify();
  }
}

impl Render for DiskCleanup {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let selected = self.rows.iter().filter(|row| row.selected && !row.moved).count();
    let bytes = self.rows.iter().filter(|row| row.selected && !row.moved).fold(0_u64, |total, row| total.saturating_add(row.candidate.bytes));
    let locked = self.busy || self.loading;
    let mut list = v_flex().gap_2();
    if self.loading {
      list = list.child(t("cleanup.loading"));
    } else if self.rows.is_empty() {
      list = list.child(t("cleanup.empty"));
    }
    for (index, row) in self.rows.iter().enumerate() {
      let path_tip = row.candidate.path.to_string_lossy().into_owned();
      let description = if row.candidate.kind.recommended() { "cleanup.cache_effect" } else { "cleanup.environment_effect" };
      let mut item = v_flex()
        .gap_1()
        .p_3()
        .border_1()
        .border_color(theme.border)
        .rounded_md()
        .child(
          h_flex()
            .gap_3()
            .child(
              Checkbox::new(SharedString::from(format!("cleanup-select-{index}")))
                .checked(row.selected)
                .disabled(locked || row.moved)
                .label(t(category(row.candidate.kind)))
                .on_click(cx.listener(move |this, selected, _, cx| {
                  this.rows[index].selected = *selected;
                  cx.notify();
                })),
            )
            .child(div().flex_1())
            .child(Tag::secondary().small().child(if row.moved { t("cleanup.moved") } else { size(row.candidate.bytes) })),
        )
        .child(
          div()
            .id(SharedString::from(format!("cleanup-path-{index}")))
            .text_xs()
            .truncate()
            .child(row.candidate.path.to_string_lossy().into_owned())
            .tooltip(move |window, cx| component::tooltip::Tooltip::new(path_tip.clone()).build(window, cx)),
        )
        .child(div().text_xs().text_color(theme.muted_foreground).child(t(description)));
      if let Some(error) = &row.error {
        item = item.child(div().text_xs().text_color(theme.danger).child(tf("cleanup.item_error", &[("error", error)])));
      }
      list = list.child(item);
    }
    let completed = self.completed;
    v_flex()
      .gap_3()
      .child(
        v_flex()
          .id("cleanup-body")
          .max_h((window.viewport_size().height - px(220.)).max(px(100.)))
          .overflow_y_scroll()
          .gap_3()
          .child(div().text_sm().child(t("cleanup.scope")))
          .child(div().text_xs().text_color(theme.muted_foreground).child(t("cleanup.note")))
          .child(
            h_flex()
              .gap_2()
              .child(Button::new("cleanup-recommended").ghost().small().label(t("cleanup.recommended")).disabled(locked).on_click(cx.listener(
                |this, _, _, cx| {
                  for row in &mut this.rows {
                    row.selected = !row.moved && row.candidate.kind.recommended();
                  }
                  cx.notify();
                },
              )))
              .child(Button::new("cleanup-clear").ghost().small().label(t("cleanup.clear")).disabled(locked).on_click(cx.listener(
                |this, _, _, cx| {
                  for row in &mut this.rows {
                    row.selected = false;
                  }
                  cx.notify();
                },
              ))),
          )
          .child(list)
          .when(completed, |view| {
            view.child(div().text_sm().child(tf("cleanup.result", &[("count", &self.rows.iter().filter(|row| row.moved).count().to_string())])))
          })
          .when_some(self.error.clone(), |view, error| {
            view.child(div().text_sm().text_color(theme.danger).child(tf("cleanup.item_error", &[("error", &error)])))
          }),
      )
      .child(div().text_sm().child(tf("cleanup.selected", &[("count", &selected.to_string()), ("size", &size(bytes))])))
      .child(
        h_flex()
          .w_full()
          .gap_2()
          .child(
            Button::new("cleanup-cancel")
              .label(t(if completed { "cleanup.close" } else { "progress.cancel" }))
              .disabled(self.busy)
              .on_click(|_, window, cx| window.close_dialog(cx)),
          )
          .child(div().flex_1())
          .child(
            Button::new("cleanup-trash")
              .primary()
              .label(t(if self.busy { "cleanup.moving" } else { "cleanup.trash" }))
              .disabled(locked || selected == 0)
              .on_click(cx.listener(|this, _, _, cx| this.move_selected(cx))),
          ),
      )
  }
}

impl Workspace {
  pub(super) fn open_disk_cleanup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    if self.model.read(cx).disk.scanning || self.model.read(cx).busy() {
      return;
    }
    let Some(index) = &self.model.read(cx).disk.index else { return };
    let proposals = index.cleanup_proposals();
    let model = self.model.clone();
    let cleanup = cx.new(|cx| DiskCleanup::new(proposals, model, cx));
    window.open_dialog(cx, move |dialog, _, _| {
      dialog.w(px(600.)).title(t("cleanup.title")).close_button(false).overlay_closable(false).keyboard(false).child(cleanup.clone())
    });
  }
}
