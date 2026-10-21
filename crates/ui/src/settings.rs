use super::{components::caption, Workspace};
use crate::platform;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    alert::Alert,
    button::{Button, ButtonVariants},
    description_list::DescriptionList,
    h_flex,
    setting::{SettingField, SettingGroup, SettingItem, SettingPage, Settings},
    spinner::Spinner,
    tag::Tag,
    v_flex, ActiveTheme, Disableable, Icon, IconName, Sizable,
  },
  *,
};
use gpui_kit_assets::IconName as Lucide;
use tendkit_i18n::{system_language, t, tf, Language};
use tendkit_model::{AppModel, AppUpdate, APP_VERSION};
use tendkit_updater::Stage;

pub(crate) const PAGE_UPDATES: usize = 1;

/// A grouped settings section with an optional explanatory footer.
pub(super) fn section(title: impl Into<SharedString>, rows: Vec<AnyElement>, footer: Option<SharedString>, cx: &App) -> impl IntoElement {
  let theme = cx.theme();
  let divider = theme.border;
  let count = rows.len();
  v_flex()
    .gap(px(6.))
    .child(div().px_1().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).child(title.into()))
    .child(
      v_flex().rounded(px(10.)).bg(theme.foreground.opacity(0.035)).border_1().border_color(theme.border.opacity(0.6)).children(
        rows
          .into_iter()
          .enumerate()
          .map(move |(index, row)| div().when(index + 1 < count, |this| this.border_b_1().border_color(divider)).px_3().py(px(9.)).child(row)),
      ),
    )
    .when_some(footer, |this, footer| this.child(caption(footer, cx).px_1()))
}

pub(super) fn labeled(label: impl Into<SharedString>, value: impl IntoElement, cx: &App) -> AnyElement {
  h_flex()
    .gap_4()
    .text_size(px(13.))
    .child(div().child(label.into()))
    .child(div().flex_1())
    .child(div().text_color(cx.theme().muted_foreground).child(value))
    .into_any_element()
}

impl Workspace {
  pub(super) fn render_settings(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let this = cx.entity();
    let model = self.model.clone();

    let shell_get = model.clone();
    let shell_set = model.clone();
    let refresh = model.clone();
    let language_get = model.clone();
    let language_set = model.clone();
    let general = SettingPage::new(t("settings.general"))
      .icon(IconName::Settings)
      .description(t("settings.general_desc"))
      .group(
        SettingGroup::new().title(t("settings.installation")).item(
          SettingItem::new(
            t("settings.path"),
            SettingField::switch(
              move |cx| shell_get.read(cx).configure_shell,
              move |value, cx| shell_set.update(cx, |model, cx| model.set_configure_shell(value, cx)),
            ),
          )
          .description(t("settings.path_desc")),
        ),
      )
      .group(
        SettingGroup::new().title(t("settings.language")).item(
          SettingItem::new(
            t("settings.language_title"),
            SettingField::dropdown(
              language_options(),
              move |cx| language_get.read(cx).language_choice().map(|language| language.code()).unwrap_or("system").into(),
              move |value, cx| {
                let choice = Language::from_code(&value);
                language_set.update(cx, |model, cx| model.set_language(choice, cx));
              },
            ),
          )
          .description(t("settings.language_desc")),
        ),
      )
      .group(
        SettingGroup::new().title(t("settings.tool_versions")).item(
          SettingItem::new(
            t("settings.refresh_title"),
            SettingField::render({
              let refresh = refresh.clone();
              move |_, _, cx| {
                let busy = refresh.read(cx).busy();
                let refresh = refresh.clone();
                Button::new("refresh-tools")
                  .label(t("action.refresh"))
                  .icon(IconName::RefreshCw)
                  .small()
                  .disabled(busy)
                  .on_click(move |_, _, cx| refresh.update(cx, |model, cx| model.refresh(cx)))
              }
            }),
          )
          .description(t("settings.refresh_desc")),
        ),
      )
      .group(
        SettingGroup::new().title(t("settings.opencommit")).item(
          SettingItem::new(
            t("settings.oc_title"),
            SettingField::render(move |_, _, _| {
              let this = this.clone();
              Button::new("opencommit-settings")
                .label(t("settings.configure"))
                .icon(Icon::new(Lucide::SlidersHorizontal))
                .small()
                .on_click(move |_, window, cx| this.update(cx, |this, cx| this.open_opencommit_settings(window, cx)))
            }),
          )
          .description(t("settings.oc_desc")),
        ),
      );

    let check_get = model.clone();
    let check_set = model.clone();
    let panel_model = model.clone();
    let updates = SettingPage::new(t("nav.updates"))
      .icon(Lucide::CircleArrowDown)
      .description(t("settings.updates_desc"))
      .group(SettingGroup::new().title("TendKit").item(SettingItem::render(move |_, _, cx| update_panel(&panel_model, cx))))
      .group(
        SettingGroup::new().title(t("settings.auto")).item(
          SettingItem::new(
            t("settings.auto_title"),
            SettingField::switch(
              move |cx| check_get.read(cx).settings.check_app_updates,
              move |value, cx| check_set.update(cx, |model, cx| model.set_check_app_updates(value, cx)),
            ),
          )
          .description(t("settings.auto_desc")),
        ),
      );

    let about_model = model.clone();
    let about = SettingPage::new(t("settings.about")).icon(IconName::Info).group(SettingGroup::new().title("TendKit").item(SettingItem::render(
      move |_, _, cx| {
        let last_scan = about_model.read(cx).last_scan.map(platform::local_time).unwrap_or_else(|| "—".into());
        DescriptionList::new()
          .columns(1)
          .bordered(false)
          .item(t("about.version"), APP_VERSION, 1)
          .item(t("about.channel"), platform::channel_label(), 1)
          .item(t("about.architecture"), platform::architecture(), 1)
          .item(platform::os_name(), platform::os_version(), 1)
          .item(t("about.last_scan"), last_scan, 1)
          .item(t("detail.source"), tendkit_updater::repository_slug().unwrap_or("—"), 1)
      },
    )));

    div().id("settings").size_full().child(
      Settings::new(SharedString::from(format!("settings-{}", self.settings_epoch)))
        .sidebar_width(px(190.))
        .default_selected_index(gpui_kit::component::setting::SelectIndex { page_ix: self.settings_page, group_ix: None })
        .pages([general, updates, about]),
    )
  }
}

/// The language picker entries: the system default first, then each language by its own name.
fn language_options() -> Vec<(SharedString, SharedString)> {
  let system = tf("settings.language_system", &[("language", system_language().native_name())]);
  std::iter::once(("system".into(), system.into()))
    .chain(Language::ALL.map(|language| (language.code().into(), language.native_name().into())))
    .collect()
}

/// The TendKit update status and its actions.
fn update_panel(model: &Entity<AppModel>, cx: &App) -> AnyElement {
  let theme = cx.theme();
  let state = model.read(cx);
  let (update, executing) = (state.app_update.clone(), state.is_executing);
  let checked = state.app_update_checked.map(platform::local_time);
  let action = |id: &'static str, label_key: &'static str, primary: bool, run: fn(&mut AppModel, &mut Context<AppModel>)| {
    let model = model.clone();
    let button = Button::new(id).label(t(label_key)).small().on_click(move |_, _, cx| model.update(cx, run));
    if primary {
      button.primary()
    } else {
      button
    }
  };
  let row = || h_flex().gap_3().items_center().flex_wrap();

  let content = match update {
    AppUpdate::Idle => row()
      .child(Tag::secondary().small().child(t("upd.not_checked")))
      .child(action("check-app-update", "action.check_updates", false, AppModel::check_app_update))
      .into_any_element(),
    AppUpdate::Checking => row().child(Spinner::new().small()).child(t("catalog.checking")).into_any_element(),
    AppUpdate::UpToDate => row()
      .child(Tag::success().small().child(t("status.up_to_date")))
      .child(caption(
        format!(
          "{}{}",
          tf("upd.version_line", &[("version", APP_VERSION)]),
          checked.map(|time| tf("upd.checked_at", &[("time", &time)])).unwrap_or_default()
        ),
        cx,
      ))
      .child(div().flex_1())
      .child(action("check-app-update", "upd.check_again", false, AppModel::check_app_update))
      .into_any_element(),
    AppUpdate::Available(release) => v_flex()
      .gap_3()
      .child(
        row()
          .child(Tag::warning().small().child(tf("upd.available", &[("version", &release.version)])))
          .child(caption(tf("upd.you_have", &[("version", APP_VERSION)]), cx))
          .child(div().flex_1())
          .when(!release.page_url.is_empty(), |this| {
            let url = release.page_url.clone();
            this.child(Button::new("release-page").ghost().small().label(t("upd.view_github")).on_click(move |_, _, cx| cx.open_url(&url)))
          })
          .child(action("download-app-update", "upd.download", true, AppModel::download_app_update)),
      )
      .when(!release.plain_notes().is_empty(), |this| {
        this.child(
          div()
            .p_3()
            .rounded(px(8.))
            .bg(theme.foreground.opacity(0.04))
            .text_size(px(12.))
            .text_color(theme.muted_foreground)
            .max_h(px(180.))
            .overflow_hidden()
            .child(release.plain_notes()),
        )
      })
      .into_any_element(),
    AppUpdate::Working(release, stage) => row()
      .child(Spinner::new().small())
      .child(match stage {
        Stage::Downloading => tf("upd.downloading", &[("version", &release.version)]),
        Stage::Verifying => t("upd.verifying"),
      })
      .into_any_element(),
    AppUpdate::Ready(release) => v_flex()
      .gap_2()
      .child(
        row()
          .child(Tag::success().small().child(tf("upd.ready", &[("version", &release.version)])))
          .child(div().flex_1())
          .child(action("restart-app-update", "upd.restart", true, AppModel::restart_to_update).disabled(executing)),
      )
      .when(executing, |this| this.child(caption(t("upd.finish_first"), cx)))
      .into_any_element(),
    AppUpdate::Failed(message) => v_flex()
      .gap_3()
      .child(Alert::error("app-update-error", message).title(t("upd.failed")))
      .child(row().child(action("check-app-update", "upd.try_again", false, AppModel::check_app_update)))
      .into_any_element(),
  };
  content
}
