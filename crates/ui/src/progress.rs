use super::{
  components::{caption, green, mono, orange},
  Workspace,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants},
    h_flex,
    progress::Progress,
    spinner::Spinner,
    v_flex, ActiveTheme, Disableable, Icon, IconName, Sizable,
  },
  *,
};
use gpui_kit_assets::IconName as Lucide;
use tendkit_protocol::{RunStatus, StepStatus};

fn status_color(status: StepStatus) -> Hsla {
  if status.is_success() {
    green()
  } else {
    orange()
  }
}

impl Workspace {
  pub(super) fn render_progress(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let model = self.model.read(cx);
    let title = if model.is_cancelling {
      "Stopping the operation"
    } else if model.is_executing {
      "Installation and updates"
    } else {
      match model.run_status {
        Some(RunStatus::Succeeded) => "Operation completed",
        Some(RunStatus::Cancelled) => "Operation cancelled",
        _ => "Some steps could not be completed",
      }
    };
    let header_icon = if model.is_executing {
      Icon::new(Lucide::Package)
    } else if model.run_status == Some(RunStatus::Succeeded) {
      Icon::new(IconName::CircleCheck)
    } else {
      Icon::new(IconName::CircleAlert)
    };
    let pending = |id: &str| {
      if model.active_step.as_deref() == Some(id) {
        "Running…"
      } else if model.run_status == Some(RunStatus::Cancelled) {
        "Not run"
      } else {
        "Pending"
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

    let header = {
      let cancel_model = self.model.clone();
      let dismiss_model = self.model.clone();
      h_flex()
        .gap_2()
        .child(header_icon.size(px(15.)))
        .child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).child(title))
        .child(div().flex_1())
        .child(if model.is_executing {
          Button::new("cancel-run")
            .label(if model.is_cancelling { "Cancelling…" } else { "Cancel" })
            .small()
            .disabled(model.is_cancelling)
            .on_click(move |_, _, cx| cancel_model.update(cx, |model, cx| model.cancel(cx)))
            .into_any_element()
        } else {
          Button::new("dismiss-run")
            .ghost()
            .xsmall()
            .icon(Icon::new(Lucide::X))
            .tooltip("Dismiss")
            .accessibility_label("Dismiss")
            .on_click(move |_, _, cx| dismiss_model.update(cx, |model, cx| model.dismiss_run_status(cx)))
            .into_any_element()
        })
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
            .child(div().text_size(px(11.)).text_color(status_color(result.status)).child(result.status.label())),
          None => h_flex().child(caption(pending(&step.id), cx)),
        };
        rows = rows
          .child(
            h_flex()
              .gap_2()
              .child(indicator(&step.id))
              .child(div().text_size(px(13.)).child(step.title.clone()))
              .child(div().flex_1())
              .child(trailing),
          )
          .when_some(result.and_then(|r| r.message.clone()), |this, message| {
            this.child(div().text_size(px(11.)).text_color(orange()).child(message))
          });
      }
      if plan.configure_shell {
        let shell = model.results.get("shell");
        rows = rows
          .child(
            h_flex()
              .gap_2()
              .child(indicator("shell"))
              .child(div().text_size(px(13.)).child("Terminal PATH"))
              .child(div().flex_1())
              .child(caption(shell.map(|r| r.status.label()).unwrap_or_else(|| pending("shell")), cx)),
          )
          .when_some(shell.and_then(|r| r.message.clone()), |this, message| this.child(div().text_size(px(11.)).text_color(orange()).child(message)));
      }
    }

    let installer = model.installer_path.is_some().then(|| {
      let open_model = self.model.clone();
      let continue_model = self.model.clone();
      v_flex()
        .gap_3()
        .pt_3()
        .border_t_1()
        .border_color(theme.border)
        .child(div().text_size(px(13.)).child("Complete the Homebrew installation in macOS Installer, then verify it here."))
        .child(
          h_flex()
            .gap_2()
            .child(
              Button::new("open-installer")
                .label("Open Installer")
                .disabled(model.is_cancelling)
                .on_click(move |_, _, cx| open_model.update(cx, |model, cx| model.open_installer(cx))),
            )
            .child(
              Button::new("continue-installer")
                .primary()
                .label("Verify and Continue")
                .disabled(model.is_cancelling)
                .on_click(move |_, _, cx| continue_model.update(cx, |model, cx| model.continue_after_installer(cx))),
            ),
        )
    });

    let footer = match model.run_status {
      Some(RunStatus::Succeeded) => Some("Tools verified. Open a new terminal session if PATH changed."),
      Some(RunStatus::Cancelled) => Some("Completed changes are preserved. Scan again to create a new plan."),
      _ => None,
    };
    let progress = model.results.len() as f32 / model.progress_total().max(1) as f32 * 100.;

    v_flex()
      .gap(px(14.))
      .p(px(18.))
      .rounded(px(12.))
      .bg(theme.blue.opacity(0.04))
      .border_1()
      .border_color(theme.blue.opacity(0.12))
      .child(header)
      .child(Progress::new("run-progress").value(progress))
      .child(rows)
      .children(installer)
      .when_some(footer, |this, text| this.child(caption(text, cx)))
  }
}
