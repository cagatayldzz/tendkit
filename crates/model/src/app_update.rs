//! TendKit's own updates: checking GitHub releases, downloading and verifying the
//! new version in the background, and restarting into it.

use crate::AppModel;
use futures::{channel::mpsc, StreamExt};
use gpui_kit::*;
use std::time::SystemTime;
use tendkit_i18n::{system_language, Language};
use tendkit_updater::{Release, Stage};

/// The version of the running application.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppUpdate {
  Idle,
  Checking,
  UpToDate,
  Available(Release),
  /// Preparing the update; the release is kept so a failure can offer a retry.
  Working(Release, Stage),
  /// Downloaded and verified; restarting installs it.
  Ready(Release),
  Failed(String),
}

impl AppUpdate {
  pub fn is_working(&self) -> bool {
    matches!(self, Self::Checking | Self::Working(..))
  }
}

enum Message<T> {
  Stage(Stage),
  Done(T),
}

impl AppModel {
  /// Looks for a newer TendKit release.
  pub fn check_app_update(&mut self, cx: &mut Context<Self>) {
    if self.app_update.is_working() || matches!(self.app_update, AppUpdate::Ready(_)) {
      return;
    }
    self.app_update = AppUpdate::Checking;
    cx.notify();
    Self::run_update_task(
      cx,
      |_| tendkit_updater::check(APP_VERSION),
      |_, _, _| {},
      |model, result, _| {
        model.app_update_checked = Some(SystemTime::now());
        model.app_update = match result {
          Ok(Some(release)) => AppUpdate::Available(release),
          Ok(None) => AppUpdate::UpToDate,
          Err(error) => AppUpdate::Failed(error.to_string()),
        };
      },
    );
  }

  /// Downloads and verifies the available release.
  pub fn download_app_update(&mut self, cx: &mut Context<Self>) {
    let release = match &self.app_update {
      AppUpdate::Available(release) => release.clone(),
      AppUpdate::Failed(_) => match &self.last_release {
        Some(release) => release.clone(),
        None => return,
      },
      _ => return,
    };
    let current = match tendkit_updater::current_app_bundle() {
      Ok(path) => path,
      Err(error) => {
        self.app_update = AppUpdate::Failed(error.to_string());
        cx.notify();
        return;
      }
    };
    self.last_release = Some(release.clone());
    self.app_update = AppUpdate::Working(release.clone(), Stage::Downloading);
    cx.notify();
    let staged_release = release.clone();
    Self::run_update_task(
      cx,
      move |stage| tendkit_updater::prepare(&release, &current, stage),
      move |model, stage, _| model.app_update = AppUpdate::Working(staged_release.clone(), stage),
      |model, result, _| match result {
        Ok(staged) => {
          let version = model.last_release.clone();
          model.staged = Some(staged);
          model.app_update = version.map(AppUpdate::Ready).unwrap_or(AppUpdate::Idle);
        }
        Err(error) => model.app_update = AppUpdate::Failed(error.to_string()),
      },
    );
  }

  /// Quits and lets the installer swap in the verified update.
  pub fn restart_to_update(&mut self, cx: &mut Context<Self>) {
    if self.is_executing {
      self.app_update = AppUpdate::Failed(tendkit_i18n::t("err.restart_busy"));
      cx.notify();
      return;
    }
    let Some(staged) = self.staged.take() else { return };
    let installed = tendkit_updater::current_app_bundle().and_then(|current| tendkit_updater::install(&staged, &current));
    match installed {
      Ok(()) => cx.quit(),
      Err(error) => {
        tendkit_updater::discard(&staged);
        self.app_update = AppUpdate::Failed(error.to_string());
        cx.notify();
      }
    }
  }

  pub fn set_check_app_updates(&mut self, value: bool, cx: &mut Context<Self>) {
    self.settings.check_app_updates = value;
    self.save_settings();
    cx.notify();
  }

  /// The saved language choice; `None` means the language of the computer.
  pub fn language_choice(&self) -> Option<Language> {
    self.settings.language.as_deref().and_then(Language::from_code)
  }

  /// Switches the interface language and remembers the choice.
  pub fn set_language(&mut self, choice: Option<Language>, cx: &mut Context<Self>) {
    self.settings.language = choice.map(|language| language.code().to_string());
    crate::apply_language(choice.unwrap_or_else(system_language));
    self.save_settings();
    cx.notify();
  }

  pub fn set_configure_shell(&mut self, value: bool, cx: &mut Context<Self>) {
    self.configure_shell = value;
    self.settings.configure_shell = value;
    self.save_settings();
    cx.notify();
  }

  fn save_settings(&mut self) {
    if let Err(error) = self.settings.save(&tendkit_storage::AppSettings::default_path()) {
      self.log.push(format!("Could not save settings: {error}"));
    }
  }

  /// Runs `work` on a background thread. `on_stage` receives progress reports on
  /// the foreground, then `done` receives the result.
  fn run_update_task<T: Send + 'static>(
    cx: &mut Context<Self>,
    work: impl FnOnce(&dyn Fn(Stage)) -> T + Send + 'static,
    on_stage: impl Fn(&mut Self, Stage, &mut Context<Self>) + 'static,
    done: impl FnOnce(&mut Self, T, &mut Context<Self>) + 'static,
  ) {
    let (sender, mut receiver) = mpsc::unbounded();
    std::thread::spawn(move || {
      let stages = sender.clone();
      let result = work(&|stage| {
        let _ = stages.unbounded_send(Message::Stage(stage));
      });
      let _ = sender.unbounded_send(Message::Done(result));
    });
    cx.spawn(async move |this, cx| {
      let mut done = Some(done);
      while let Some(message) = receiver.next().await {
        let alive = this.update(cx, |model, cx| {
          match message {
            Message::Stage(stage) => on_stage(model, stage, cx),
            Message::Done(result) => {
              if let Some(done) = done.take() {
                done(model, result, cx);
              }
            }
          }
          cx.notify();
        });
        if alive.is_err() {
          break;
        }
      }
    })
    .detach();
  }
}
