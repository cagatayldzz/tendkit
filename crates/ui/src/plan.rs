use super::{
  components::{caption, green, mono},
  Workspace,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants},
    h_flex, v_flex, ActiveTheme, Icon, IconName, WindowExt,
  },
  *,
};
use gpui_kit_assets::IconName as Lucide;
use std::{cell::Cell, rc::Rc};
use tendkit_protocol::{InstallStep, Recipe, StepAction};

fn source_label(step: &InstallStep) -> &'static str {
  match &step.recipe {
    Recipe::Homebrew | Recipe::Brew { .. } => "Homebrew",
    Recipe::Opencommit { manager } if manager == "bun" => "Bun",
    Recipe::Opencommit { .. } => "npm",
    Recipe::Bun | Recipe::Codex | Recipe::Claude { .. } => "Official installer",
  }
}

impl Workspace {
  pub(super) fn open_plan(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let model = self.model.clone();
    let paths_expanded = Rc::new(Cell::new(false));
    window.open_dialog(cx, move |dialog, _, cx| {
      let Some(plan) = model.read(cx).plan.clone() else { return dialog };
      let theme = cx.theme().clone();
      let dismiss = {
        let model = model.clone();
        move |_: &ClickEvent, _: &mut Window, cx: &mut App| model.update(cx, |model, cx| model.dismiss_plan(cx))
      };
      let mut steps = v_flex().gap_4();
      for step in &plan.steps {
        let skip = step.action == StepAction::Skip;
        steps = steps.child(
          h_flex()
            .items_start()
            .gap_3()
            .child(if skip { Icon::new(IconName::CircleCheck) } else { Icon::new(Lucide::CircleArrowDown) }.size(px(16.)).text_color(if skip {
              green()
            } else {
              theme.blue
            }))
            .child(
              v_flex()
                .flex_1()
                .gap_1()
                .child(div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).child(step.title.clone()))
                .child(caption(step.action_label(), cx))
                .when(step.id != "homebrew" || step.action == StepAction::Install, |this| {
                  let before = step.before.as_ref().and_then(|i| i.version.clone()).unwrap_or_else(|| "Not installed".into());
                  this.child(mono(format!("{before} → {}", step.target)).text_size(px(11.)))
                })
                .when(!plan.selected.contains(&step.id), |this| this.child(caption("Required dependency", cx).text_size(px(10.)))),
            )
            .child(caption(source_label(step), cx)),
        );
      }
      if plan.configure_shell {
        let expanded = paths_expanded.get();
        let toggle = paths_expanded.clone();
        steps = steps
          .child(div().h(px(1.)).bg(theme.border))
          .child(
            h_flex()
              .gap_2()
              .text_size(px(13.))
              .font_weight(FontWeight::MEDIUM)
              .child(Icon::new(Lucide::Terminal).size(px(14.)))
              .child("Terminal PATH settings"),
          )
          .child(caption(
            "Files are backed up and only the TendKit block is updated. Directories are appended, so existing PATH entries keep priority over them.",
            cx,
          ))
          .children(plan.shell_files.iter().map(|file| mono(file.clone()).text_size(px(10.))))
          .child(
            h_flex()
              .id("shell-paths")
              .gap_1()
              .cursor_pointer()
              .text_size(px(11.))
              .child(Icon::new(if expanded { IconName::ChevronDown } else { IconName::ChevronRight }).size(px(11.)))
              .child("Directories to add to PATH")
              .on_click(move |_, window, _| {
                toggle.set(!toggle.get());
                window.refresh();
              }),
          )
          .when(expanded, |this| this.children(plan.shell_paths.iter().map(|path| mono(path.clone()).pl_4().text_size(px(10.)))));
      }
      if plan.package.is_some() {
        steps = steps.child(
          h_flex()
            .items_start()
            .gap_2()
            .text_color(theme.muted_foreground)
            .child(Icon::new(Lucide::Package).size(px(12.)))
            .child(caption("Homebrew is installed through macOS Installer. Any required password is requested by the operating system.", cx)),
        );
      }
      if plan.selected.iter().any(|id| id == "opencommit") {
        steps =
          steps.child(caption("OpenCommit settings are preserved. Configure your provider and API key separately after a new installation.", cx));
      }
      steps = steps.child(caption("The package manager may also install required dependencies. Cancelling does not undo completed operations.", cx));

      let cancel = dismiss.clone();
      let approve_model = model.clone();
      dialog
        .w(px(550.))
        .title(
          h_flex()
            .gap_2()
            .child(Icon::new(Lucide::Package).size(px(18.)))
            .child(div().text_size(px(19.)).font_weight(FontWeight::SEMIBOLD).child("Installation plan")),
        )
        .on_cancel(move |event, window, cx| {
          cancel(event, window, cx);
          true
        })
        .on_close(dismiss)
        .child(
          v_flex()
            .gap_4()
            .child(
              div()
                .text_size(px(13.))
                .text_color(theme.muted_foreground)
                .child("Selected tools and required dependencies will be processed in the order below."),
            )
            .child(div().id("plan-steps").max_h(px(360.)).overflow_y_scroll().py_1().child(steps)),
        )
        .footer(
          h_flex()
            .w_full()
            .child(Button::new("plan-cancel").label("Cancel").on_click(|_, window, cx| window.close_dialog(cx)))
            .child(div().flex_1())
            .child(Button::new("plan-approve").primary().label("Approve and Start").on_click(move |_, window, cx| {
              approve_model.update(cx, |model, cx| model.execute(cx));
              window.close_dialog(cx);
            })),
        )
    });
  }
}
