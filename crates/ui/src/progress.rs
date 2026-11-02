use super::{
  components::{caption, green, mono, orange, step_status_label},
  Workspace,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    alert::Alert,
    button::{Button, ButtonVariants},
    h_flex,
    progress::Progress,
    spinner::Spinner,
    v_flex, ActiveTheme, Disableable, Icon, IconName, Sizable, WindowExt,
  },
  *,
};
use gpui_kit_assets::IconName as Lucide;
use tendkit_i18n::t;
use tendkit_model::AppModel;
use tendkit_protocol::{RunStatus, StepStatus};

fn status_color(status: StepStatus) -> Hsla {
  if status.is_success() {
    green()
  } else {
    orange()
  }
}

impl Workspace {
  /// Opens the run progress as a dialog. It stays open while the plan runs, can
  /// only be left through Cancel, and is dismissed once the run has finished.
  pub(super) fn open_progress(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    self.progress_open = true;
    let model = self.model.clone();
    let workspace = cx.entity();
    window.open_dialog(cx, move |dialog, _, cx| {
      let state = model.read(cx);
      let executing = state.is_executing;
      let cancelling = state.is_cancelling;
      let title = progress_title(state);
      let header_icon = if executing {
        Icon::new(Lucide::Package)
      } else if state.run_status == Some(RunStatus::Succeeded) {
        Icon::new(IconName::CircleCheck)
      } else {
        Icon::new(IconName::CircleAlert)
      };
      let close = {
        let model = model.clone();
        let workspace = workspace.clone();
        move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
          model.update(cx, |model, cx| model.dismiss_run_status(cx));
          workspace.update(cx, |this, _| this.progress_open = false);
        }
      };
      let on_cancel = close.clone();
      let footer = if executing {
        let cancel_model = model.clone();
        Button::new("cancel-run")
          .label(if cancelling { t("activity.cancelling") } else { t("progress.cancel") })
          .disabled(cancelling)
          .on_click(move |_, _, cx| cancel_model.update(cx, |model, cx| model.cancel(cx)))
          .into_any_element()
      } else {
        Button::new("dismiss-run").primary().label(t("progress.close")).on_click(|_, window, cx| window.close_dialog(cx)).into_any_element()
      };
      dialog
        .w(px(550.))
        .title(h_flex().gap_2().child(header_icon.size(px(18.))).child(div().text_size(px(19.)).font_weight(FontWeight::SEMIBOLD).child(title)))
        // Closing a running operation would hide it; Cancel is the way out.
        .close_button(!executing)
        .overlay_closable(!executing)
        .keyboard(!executing)
        .on_cancel(move |event, window, cx| {
          on_cancel(event, window, cx);
          true
        })
        .on_close(close)
        .child(progress_body(&model, cx))
        .footer(h_flex().w_full().child(div().flex_1()).child(footer))
    });
  }
}

fn progress_title(model: &AppModel) -> String {
  if model.is_cancelling {
    t("progress.title_stopping")
  } else if model.is_executing {
    t("progress.title_running")
  } else {
    t(match model.run_status {
      Some(RunStatus::Succeeded) => "progress.title_done",
      Some(RunStatus::Cancelled) => "progress.title_cancelled",
      _ => "progress.title_failed",
    })
  }
}

/// The step list, installer prompt, and outcome of the current run.
fn progress_body(model_entity: &Entity<AppModel>, cx: &App) -> impl IntoElement {
  let theme = cx.theme().clone();
  let model = model_entity.read(cx);
  let pending = |id: &str| {
    if model.active_step.as_deref() == Some(id) {
      t("progress.running")
    } else if model.run_status == Some(RunStatus::Cancelled) {
      t("progress.not_run")
    } else {
      t("progress.pending")
    }
  };
  let indicator = |id: &str| -> AnyElement {
    if model.active_step.as_deref() == Some(id) {
      div().w(px(16.)).child(Spinner::new().xsmall()).into_any_element()
    } else {
      let (icon, color) = match model.results.get(id) {
        Some(result) if result.status.is_success() => (Icon::new(IconName::CircleCheck), status_color(result.status)),
        Some(result) => (Icon::new(IconName::CircleAlert), status_color(result.status)),
        None => (Icon::new(Lucide::Circle), theme.muted_foreground),
      };
      div().w(px(16.)).child(icon.size(px(14.)).text_color(color)).into_any_element()
    }
  };

  let mut rows = v_flex().gap(px(10.));
  if let Some(plan) = &model.plan {
    for step in &plan.steps {
      let result = model.results.get(&step.id);
      let trailing = match result {
        Some(result) => h_flex()
          .gap_2()
          .when_some(result.before.clone().zip(result.after.clone()).filter(|(b, a)| b != a), |this, (before, after)| {
            this.child(mono(format!("{before} → {after}")).text_size(px(11.)).text_color(theme.muted_foreground))
          })
          .child(div().text_size(px(11.)).text_color(status_color(result.status)).child(step_status_label(result.status))),
        None => h_flex().child(caption(pending(&step.id), cx)),
      };
      rows = rows
        .child(
          h_flex().gap_2().child(indicator(&step.id)).child(div().text_size(px(13.)).child(step.title.clone())).child(div().flex_1()).child(trailing),
        )
        .when_some(result.and_then(|r| r.message.clone()), |this, message| this.child(div().text_size(px(11.)).text_color(orange()).child(message)));
    }
    if plan.configure_shell {
      let shell = model.results.get("shell");
      rows = rows
        .child(
          h_flex()
            .gap_2()
            .child(indicator("shell"))
            .child(div().text_size(px(13.)).child(t("progress.terminal_path")))
            .child(div().flex_1())
            .child(caption(shell.map(|r| step_status_label(r.status)).unwrap_or_else(|| pending("shell")), cx)),
        )
        .when_some(shell.and_then(|r| r.message.clone()), |this, message| this.child(div().text_size(px(11.)).text_color(orange()).child(message)));
    }
  }

  let installer = model.installer_path.is_some().then(|| {
    let open_model = model_entity.clone();
    let continue_model = model_entity.clone();
    v_flex().gap_3().pt_3().border_t_1().border_color(theme.border).child(div().text_size(px(13.)).child(t("progress.installer_note"))).child(
      h_flex()
        .gap_2()
        .child(
          Button::new("open-installer")
            .label(t("progress.open_installer"))
            .disabled(model.is_cancelling)
            .on_click(move |_, _, cx| open_model.update(cx, |model, cx| model.open_installer(cx))),
        )
        .child(
          Button::new("continue-installer")
            .primary()
            .label(t("progress.verify"))
            .disabled(model.is_cancelling)
            .on_click(move |_, _, cx| continue_model.update(cx, |model, cx| model.continue_after_installer(cx))),
        ),
    )
  });

  let footer = match model.run_status {
    Some(RunStatus::Succeeded) => Some(t("progress.footer_ok")),
    Some(RunStatus::Cancelled) => Some(t("progress.footer_cancelled")),
    _ => None,
  };
  let progress = model.results.len() as f32 / model.progress_total().max(1) as f32 * 100.;

  v_flex()
    .gap(px(14.))
    .child(Progress::new("run-progress").value(progress))
    .child(div().id("run-steps").max_h(px(360.)).overflow_y_scroll().child(rows))
    .children(installer)
    .when_some(model.error.clone().filter(|_| !model.is_executing), |this, error| this.child(Alert::error("run-error", error)))
    .when_some(footer, |this, text| this.child(caption(text, cx)))
}
