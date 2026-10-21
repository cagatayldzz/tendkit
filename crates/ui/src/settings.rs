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
use tendkit_model::{AppModel, AppUpdate, APP_VERSION};
use tendkit_updater::Stage;

pub(crate) const PAGE_UPDATES: usize = 1;

/// A grouped settings section with an optional explanatory footer.
pub(super) fn section(title: &'static str, rows: Vec<AnyElement>, footer: Option<SharedString>, cx: &App) -> impl IntoElement {
  let theme = cx.theme();
  let divider = theme.border;
  let count = rows.len();
  v_flex()
    .gap(px(6.))
    .child(div().px_1().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).child(title))
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
    let general = SettingPage::new("General")
      .icon(IconName::Settings)
      .description("How TendKit installs and checks your developer tools.")
      .group(
        SettingGroup::new().title("Installation").item(
          SettingItem::new(
            "Configure terminal PATH",
            SettingField::switch(
              move |cx| shell_get.read(cx).configure_shell,
              move |value, cx| shell_set.update(cx, |model, cx| model.set_configure_shell(value, cx)),
            ),
          )
          .description(
            "Add managed tools to your PATH when you approve an installation plan. Directories are appended, so existing PATH entries keep priority.",
          ),
        ),
      )
      .group(
        SettingGroup::new().title("Tool versions").item(
          SettingItem::new(
            "Refresh tool information",
            SettingField::render({
              let refresh = refresh.clone();
              move |_, _, cx| {
                let busy = refresh.read(cx).busy();
                let refresh = refresh.clone();
                Button::new("refresh-tools")
                  .label("Refresh")
                  .icon(IconName::RefreshCw)
                  .small()
                  .disabled(busy)
                  .on_click(move |_, _, cx| refresh.update(cx, |model, cx| model.refresh(cx)))
              }
            }),
          )
          .description("Clears cached versions, rescans your tools, and checks for updates. Latest versions are cached for 6 hours."),
        ),
      )
      .group(
        SettingGroup::new().title("OpenCommit").item(
          SettingItem::new(
            "Commit message style",
            SettingField::render(move |_, _, _| {
              let this = this.clone();
              Button::new("opencommit-settings")
                .label("Configure…")
                .icon(Icon::new(Lucide::SlidersHorizontal))
                .small()
                .on_click(move |_, window, cx| this.update(cx, |this, cx| this.open_opencommit_settings(window, cx)))
            }),
          )
          .description("Model, language, and commit instructions. Available when OpenCommit is installed."),
        ),
      );

    let check_get = model.clone();
    let check_set = model.clone();
    let panel_model = model.clone();
    let updates = SettingPage::new("Updates")
      .icon(Lucide::CircleArrowDown)
      .description("Keep TendKit itself up to date. New versions come from GitHub releases.")
      .group(SettingGroup::new().title("TendKit").item(SettingItem::render(move |_, _, cx| update_panel(&panel_model, cx))))
      .group(
        SettingGroup::new().title("Automatic checks").item(
          SettingItem::new(
            "Check for updates on launch",
            SettingField::switch(
              move |cx| check_get.read(cx).settings.check_app_updates,
              move |value, cx| check_set.update(cx, |model, cx| model.set_check_app_updates(value, cx)),
            ),
          )
          .description(
            "TendKit only downloads an update when you ask. Every download is verified against its published checksum and code signature.",
          ),
        ),
      );

    let about_model = model.clone();
    let about =
      SettingPage::new("About").icon(IconName::Info).group(SettingGroup::new().title("TendKit").item(SettingItem::render(move |_, _, cx| {
        let last_scan = about_model.read(cx).last_scan.map(platform::local_time).unwrap_or_else(|| "—".into());
        DescriptionList::new()
          .columns(1)
          .bordered(false)
          .item("Version", APP_VERSION, 1)
          .item("Channel", platform::channel(), 1)
          .item("Architecture", platform::architecture(), 1)
          .item(platform::os_name(), platform::os_version(), 1)
          .item("Last scan", last_scan, 1)
          .item("Source", tendkit_updater::repository_slug().unwrap_or("—"), 1)
      })));

    div().id("settings").size_full().child(
      Settings::new(SharedString::from(format!("settings-{}", self.settings_epoch)))
        .sidebar_width(px(190.))
        .default_selected_index(gpui_kit::component::setting::SelectIndex { page_ix: self.settings_page, group_ix: None })
        .pages([general, updates, about]),
    )
  }
}

/// The TendKit update status and its actions.
fn update_panel(model: &Entity<AppModel>, cx: &App) -> AnyElement {
  let theme = cx.theme();
  let state = model.read(cx);
  let (update, executing) = (state.app_update.clone(), state.is_executing);
  let checked = state.app_update_checked.map(platform::local_time);
  let action = |id: &'static str, label: &'static str, primary: bool, run: fn(&mut AppModel, &mut Context<AppModel>)| {
    let model = model.clone();
    let button = Button::new(id).label(label).small().on_click(move |_, _, cx| model.update(cx, run));
    if primary {
      button.primary()
    } else {
      button
    }
  };
  let row = || h_flex().gap_3().items_center().flex_wrap();

  let content = match update {
    AppUpdate::Idle => row()
      .child(Tag::secondary().small().child("Not checked yet"))
      .child(action("check-app-update", "Check for Updates", false, AppModel::check_app_update))
      .into_any_element(),
    AppUpdate::Checking => row().child(Spinner::new().small()).child("Checking for updates…").into_any_element(),
    AppUpdate::UpToDate => row()
      .child(Tag::success().small().child("Up to date"))
      .child(caption(format!("TendKit {APP_VERSION}{}", checked.map(|t| format!(" · checked at {t}")).unwrap_or_default()), cx))
      .child(div().flex_1())
      .child(action("check-app-update", "Check Again", false, AppModel::check_app_update))
      .into_any_element(),
    AppUpdate::Available(release) => v_flex()
      .gap_3()
      .child(
        row()
          .child(Tag::warning().small().child(format!("Version {} available", release.version)))
          .child(caption(format!("You have {APP_VERSION}"), cx))
          .child(div().flex_1())
          .when(!release.page_url.is_empty(), |this| {
            let url = release.page_url.clone();
            this.child(Button::new("release-page").ghost().small().label("View on GitHub").on_click(move |_, _, cx| cx.open_url(&url)))
          })
          .child(action("download-app-update", "Download & Install", true, AppModel::download_app_update)),
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
        Stage::Downloading => format!("Downloading TendKit {}…", release.version),
        Stage::Verifying => "Verifying checksum and signature…".to_string(),
      })
      .into_any_element(),
    AppUpdate::Ready(release) => v_flex()
      .gap_2()
      .child(
        row()
          .child(Tag::success().small().child(format!("Version {} is ready", release.version)))
          .child(div().flex_1())
          .child(action("restart-app-update", "Restart to Update", true, AppModel::restart_to_update).disabled(executing)),
      )
      .when(executing, |this| this.child(caption("Finish or cancel the running installation first.", cx)))
      .into_any_element(),
    AppUpdate::Failed(message) => v_flex()
      .gap_3()
      .child(Alert::error("app-update-error", message).title("Update failed"))
      .child(row().child(action("check-app-update", "Try Again", false, AppModel::check_app_update)))
      .into_any_element(),
  };
  content
}
