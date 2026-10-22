use super::{components::orange, Destination, Workspace};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{spinner::Spinner, status_bar::StatusBar, tag::Tag, Sizable},
  *,
};
use tendkit_i18n::{t, tf};
use tendkit_model::{AppUpdate, APP_UPDATES_ENABLED};

impl Workspace {
  /// The bottom bar: what the app is doing, and the app update state.
  pub(super) fn render_status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let model = self.model.read(cx);
    let activity = if model.is_executing {
      Some(t(if model.is_cancelling { "activity.cancelling" } else { "activity.applying" }))
    } else if model.is_planning {
      Some(t("activity.planning"))
    } else if model.is_scanning {
      Some(t("activity.scanning"))
    } else if model.is_checking_updates() {
      Some(t("activity.checking_updates"))
    } else {
      None
    };
    let (installed, total, updates) = (model.installed_count(), model.tools.len(), model.update_count());
    let app_update = model.app_update.clone();
    let idle = activity.is_none();

    StatusBar::new()
      .left(match activity {
        Some(text) => div().flex().items_center().gap_2().child(Spinner::new().xsmall()).child(text).into_any_element(),
        None => {
          div().child(tf("statusbar.installed_count", &[("installed", &installed.to_string()), ("total", &total.to_string())])).into_any_element()
        }
      })
      .when(updates > 0 && idle, |bar| bar.left(div().text_color(orange()).child(tf("statusbar.tool_updates", &[("count", &updates.to_string())]))))
      .when(APP_UPDATES_ENABLED, |bar| {
        bar.right(match app_update {
          AppUpdate::Available(release) => {
            Self::update_tag(Tag::warning().small().child(tf("statusbar.app_available", &[("version", &release.version)])), cx)
          }
          AppUpdate::Ready(_) => Self::update_tag(Tag::success().small().child(t("statusbar.restart")), cx),
          _ => div().into_any_element(),
        })
      })
      .when(self.destination != Destination::Settings, |bar| bar.right(div().child(crate::platform::channel_label())))
  }

  /// Makes the update tag open Settings → Updates.
  fn update_tag(tag: Tag, cx: &mut Context<Self>) -> AnyElement {
    div()
      .id("app-update-tag")
      .cursor_pointer()
      .on_click(cx.listener(|this, _, _, cx| this.open_settings(crate::settings::PAGE_UPDATES, cx)))
      .child(tag)
      .into_any_element()
  }
}
