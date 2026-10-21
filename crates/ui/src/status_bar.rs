use super::{components::orange, Destination, Workspace};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{spinner::Spinner, status_bar::StatusBar, tag::Tag, Sizable},
  *,
};
use tendkit_model::AppUpdate;

impl Workspace {
  /// The bottom bar: what the app is doing, and the app update state.
  pub(super) fn render_status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let model = self.model.read(cx);
    let activity = if model.is_executing {
      Some(if model.is_cancelling { "Cancelling…" } else { "Applying changes…" })
    } else if model.is_planning {
      Some("Preparing plan…")
    } else if model.is_scanning {
      Some("Scanning tools…")
    } else if model.is_checking_updates() {
      Some("Checking for tool updates…")
    } else {
      None
    };
    let (installed, total, updates) = (model.installed_count(), model.tools.len(), model.update_count());
    let app_update = model.app_update.clone();

    StatusBar::new()
      .left(match activity {
        Some(text) => div().flex().items_center().gap_2().child(Spinner::new().xsmall()).child(text).into_any_element(),
        None => div().child(format!("{installed} of {total} tools installed")).into_any_element(),
      })
      .when(updates > 0 && activity.is_none(), |bar| bar.left(div().text_color(orange()).child(format!("{updates} tool updates"))))
      .right(match app_update {
        AppUpdate::Available(release) => Self::update_tag(Tag::warning().small().child(format!("TendKit {} available", release.version)), cx),
        AppUpdate::Ready(_) => Self::update_tag(Tag::success().small().child("Restart to update"), cx),
        _ => div().into_any_element(),
      })
      .when(self.destination != Destination::Settings, |bar| bar.right(div().child(crate::platform::channel())))
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
