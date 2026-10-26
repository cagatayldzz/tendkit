use super::{disk::size, platform};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants},
    h_flex,
    input::{Input, InputEvent, InputState},
    menu::{ContextMenuExt, PopupMenuItem},
    tag::Tag,
    v_flex, ActiveTheme, Disableable, Icon, IconName, Selectable, Sizable, WindowExt,
  },
  *,
};
use gpui_kit_assets::IconName as Lucide;
use std::{collections::HashSet, path::PathBuf};
use tendkit_i18n::{t, tf};
use tendkit_model::AppModel;
use tendkit_storage::applications::{self, ApplicationInventory, ApplicationOrigin, InstalledApplication, RemovalBlock};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Filter {
  All,
  Removable,
  Protected,
}

pub(super) struct ApplicationsPage {
  model: Entity<AppModel>,
  inventory: ApplicationInventory,
  running: HashSet<PathBuf>,
  search: Entity<InputState>,
  filter: Filter,
  loaded: bool,
  scanning: bool,
  removing: Option<PathBuf>,
  visible: usize,
  error: Option<String>,
  message: Option<String>,
  _subscriptions: Vec<Subscription>,
}

fn block_key(block: RemovalBlock) -> &'static str {
  match block {
    RemovalBlock::System => "apps.system_reason",
    RemovalBlock::SymbolicLink => "apps.link_reason",
    RemovalBlock::InvalidBundle => "apps.invalid_reason",
    RemovalBlock::CurrentApplication => "apps.self_reason",
  }
}

impl ApplicationsPage {
  pub(super) fn new(model: Entity<AppModel>, window: &mut Window, cx: &mut Context<Self>) -> Self {
    let search = cx.new(|cx| InputState::new(window, cx).placeholder(t("apps.search")));
    let subscription = cx.subscribe(&search, |this, _, event: &InputEvent, cx| {
      if matches!(event, InputEvent::Change) {
        this.visible = 100;
        cx.notify();
      }
    });
    let model_subscription = cx.observe(&model, |_, _, cx| cx.notify());
    Self {
      model,
      inventory: ApplicationInventory::default(),
      running: HashSet::new(),
      search,
      filter: Filter::All,
      loaded: false,
      scanning: false,
      removing: None,
      visible: 100,
      error: None,
      message: None,
      _subscriptions: vec![subscription, model_subscription],
    }
  }

  pub(super) fn ensure_loaded(&mut self, cx: &mut Context<Self>) {
    if !self.loaded {
      self.scan(cx);
    }
  }

  fn scan(&mut self, cx: &mut Context<Self>) {
    if self.scanning || self.removing.is_some() || !cfg!(target_os = "macos") {
      return;
    }
    self.scanning = true;
    self.error = None;
    let task = cx.background_executor().spawn(async move { (applications::scan_installed(), platform::running_application_paths()) });
    cx.spawn(async move |this, cx| {
      let (inventory, running) = task.await;
      let _ = this.update(cx, |this, cx| {
        this.inventory = inventory;
        this.running = running;
        this.loaded = true;
        this.scanning = false;
        this.visible = 100;
        cx.notify();
      });
    })
    .detach();
    cx.notify();
  }

  fn confirm_removal(&mut self, app: InstalledApplication, window: &mut Window, cx: &mut Context<Self>) {
    if app.blocked.is_some() || self.scanning || self.removing.is_some() || self.model.read(cx).busy() {
      return;
    }
    let page = cx.entity().downgrade();
    window.open_dialog(cx, move |dialog, _, _| {
      let page = page.clone();
      let app_to_remove = app.clone();
      dialog
        .w(px(480.))
        .title(tf("apps.confirm_title", &[("name", &app.name)]))
        .child(
          v_flex()
            .gap_3()
            .child(div().text_sm().child(t("apps.confirm_note")))
            .child(div().text_xs().child(app.path.to_string_lossy().into_owned()))
            .child(div().text_sm().child(app.bytes.map(size).unwrap_or_else(|| t("apps.size_unknown")))),
        )
        .footer(
          h_flex()
            .w_full()
            .gap_2()
            .child(Button::new("app-remove-cancel").label(t("progress.cancel")).on_click(|_, window, cx| window.close_dialog(cx)))
            .child(div().flex_1())
            .child(Button::new("app-remove-confirm").primary().label(t("apps.trash")).on_click(move |_, window, cx| {
              let _ = page.update(cx, |page, cx| page.remove(app_to_remove.clone(), cx));
              window.close_dialog(cx);
            })),
        )
    });
  }

  fn remove(&mut self, app: InstalledApplication, cx: &mut Context<Self>) {
    if self.removing.is_some() || self.scanning || self.model.read(cx).busy() {
      return;
    }
    self.removing = Some(app.path.clone());
    self.error = None;
    self.message = None;
    let task_app = app.clone();
    let task = cx.background_executor().spawn(async move {
      let current = std::env::current_exe().ok();
      applications::move_to_trash(&task_app, &tendkit_storage::paths::home_dir(), current.as_deref(), platform::trash_application)
    });
    cx.spawn(async move |this, cx| {
      let result = task.await;
      let _ = this.update(cx, |this, cx| {
        this.removing = None;
        match result {
          Ok(()) => {
            this.inventory.applications.retain(|item| item.path != app.path);
            this.message = Some(tf("apps.removed", &[("name", &app.name)]));
          }
          Err(error) => this.error = Some(tf("apps.remove_error", &[("name", &app.name), ("error", &error)])),
        }
        this.running = platform::running_application_paths();
        cx.notify();
      });
    })
    .detach();
    cx.notify();
  }
}

impl Render for ApplicationsPage {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    if !cfg!(target_os = "macos") {
      return div().p_5().child(t("apps.unsupported")).into_any_element();
    }
    let theme = cx.theme().clone();
    let busy = self.scanning || self.removing.is_some();
    let query = self.search.read(cx).value().trim().to_lowercase();
    let protected = self.inventory.applications.iter().filter(|app| app.blocked.is_some()).count();
    let mut filters = h_flex().gap_2().flex_wrap();
    for (filter, key) in [(Filter::All, "apps.all"), (Filter::Removable, "apps.removable"), (Filter::Protected, "apps.protected")] {
      filters = filters.child(Button::new(SharedString::from(key)).ghost().small().selected(self.filter == filter).label(t(key)).on_click(
        cx.listener(move |this, _, _, cx| {
          this.filter = filter;
          this.visible = 100;
          cx.notify();
        }),
      ));
    }
    let apps: Vec<_> = self
      .inventory
      .applications
      .iter()
      .filter(|app| {
        (self.filter == Filter::All || (self.filter == Filter::Protected) == app.blocked.is_some())
          && (app.name.to_lowercase().contains(&query)
            || app.path.to_string_lossy().to_lowercase().contains(&query)
            || app.bundle_id.as_ref().is_some_and(|id| id.to_lowercase().contains(&query)))
      })
      .collect();
    let mut list = v_flex().w_full();
    for (index, app) in apps.iter().take(self.visible).enumerate() {
      let is_running = self.running.contains(&app.path);
      let removing = self.removing.as_ref() == Some(&app.path);
      let reveal = app.path.clone();
      let context_path = app.path.clone();
      let app_to_remove = (*app).clone();
      let reason = app.blocked.map(block_key);
      let path_tip = app.path.to_string_lossy().into_owned();
      list = list.child(
        h_flex()
          .id(SharedString::from(format!("application-{index}")))
          .w_full()
          .gap_3()
          .px_5()
          .py_3()
          .border_b_1()
          .border_color(theme.border)
          .child(
            Icon::new(if app.blocked == Some(RemovalBlock::System) { Lucide::ShieldCheck } else { Lucide::AppWindow })
              .size(px(28.))
              .text_color(theme.blue),
          )
          .child(
            v_flex()
              .flex_1()
              .min_w_0()
              .gap_1()
              .child(
                h_flex()
                  .gap_2()
                  .flex_wrap()
                  .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(app.name.clone()))
                  .when_some(app.version.clone(), |row, version| row.child(div().text_xs().text_color(theme.muted_foreground).child(version)))
                  .when(app.blocked.is_some(), |row| row.child(Tag::secondary().small().child(t("apps.protected"))))
                  .when(is_running, |row| row.child(Tag::info().small().child(t("apps.running")))),
              )
              .child(
                div()
                  .id(SharedString::from(format!("application-path-{index}")))
                  .text_xs()
                  .text_color(theme.muted_foreground)
                  .truncate()
                  .child(app.path.to_string_lossy().into_owned())
                  .tooltip(move |window, cx| component::tooltip::Tooltip::new(path_tip.clone()).build(window, cx)),
              )
              .child(div().text_xs().text_color(theme.muted_foreground).child(t(match app.origin {
                ApplicationOrigin::System => "apps.origin_system",
                ApplicationOrigin::Shared => "apps.origin_shared",
                ApplicationOrigin::User => "apps.origin_user",
              })))
              .when_some(reason, |column, reason| column.child(div().text_xs().text_color(theme.muted_foreground).child(t(reason)))),
          )
          .child(div().w(px(90.)).text_right().text_sm().child(app.bytes.map(size).unwrap_or_else(|| "—".into())))
          .child(
            Button::new(SharedString::from(format!("application-reveal-{index}")))
              .ghost()
              .small()
              .icon(Lucide::Folder)
              .tooltip(t("disk.reveal"))
              .on_click(move |_, _, cx| cx.reveal_path(&reveal)),
          )
          .child(
            Button::new(SharedString::from(format!("application-remove-{index}")))
              .ghost()
              .small()
              .icon(Lucide::Trash)
              .label(t(if removing { "apps.moving" } else { "apps.remove" }))
              .tooltip(t(reason.unwrap_or(if is_running { "apps.running_reason" } else { "apps.remove_tip" })))
              .disabled(busy || app.blocked.is_some() || is_running || self.model.read(cx).busy())
              .on_click(cx.listener(move |this, _, window, cx| this.confirm_removal(app_to_remove.clone(), window, cx))),
          )
          .context_menu(move |menu, _, _| {
            let open = context_path.clone();
            let reveal = context_path.clone();
            let copy = context_path.clone();
            menu
              .item(PopupMenuItem::new(t("apps.open_app")).icon(Lucide::AppWindow).on_click(move |_, _, cx| cx.open_with_system(&open)))
              .item(PopupMenuItem::new(t("disk.reveal")).icon(Lucide::Folder).on_click(move |_, _, cx| cx.reveal_path(&reveal)))
              .item(
                PopupMenuItem::new(t("disk.copy_path"))
                  .icon(Lucide::Copy)
                  .on_click(move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(copy.to_string_lossy().into_owned()))),
              )
          }),
      );
    }
    let shown = apps.len();
    if !self.scanning && apps.is_empty() {
      list = list.child(div().p_6().text_center().text_color(theme.muted_foreground).child(t("apps.empty")));
    }
    if shown > self.visible {
      list = list.child(div().p_3().child(Button::new("applications-more").ghost().label(t("disk.more")).on_click(cx.listener(|this, _, _, cx| {
        this.visible += 100;
        cx.notify();
      }))));
    }
    v_flex()
      .size_full()
      .child(
        v_flex()
          .w_full()
          .gap_3()
          .p_5()
          .border_b_1()
          .border_color(theme.border)
          .child(
            h_flex()
              .gap_3()
              .child(
                div()
                  .flex_1()
                  .text_sm()
                  .child(tf("apps.summary", &[("count", &self.inventory.applications.len().to_string()), ("protected", &protected.to_string())])),
              )
              .child(
                Button::new("applications-refresh")
                  .ghost()
                  .small()
                  .icon(IconName::RefreshCw)
                  .label(t("apps.refresh"))
                  .disabled(busy)
                  .on_click(cx.listener(|this, _, _, cx| this.scan(cx))),
              ),
          )
          .child(
            h_flex().gap_3().child(
              div()
                .flex_1()
                .child(Input::new(&self.search).with_size(component::Size::Medium).prefix(Icon::new(IconName::Search).small()).cleanable(true)),
            ),
          )
          .child(filters)
          .child(div().text_xs().text_color(theme.muted_foreground).child(t("apps.scope")))
          .child(
            h_flex().gap_2().child(Icon::new(Lucide::ShieldCheck).small()).child(div().flex_1().text_xs().child(t("apps.protection_note"))).child(
              Button::new("applications-help")
                .ghost()
                .small()
                .label(t("apps.help"))
                .on_click(|_, _, cx| cx.open_url("https://support.apple.com/102610")),
            ),
          )
          .when(self.inventory.incomplete, |view| view.child(div().text_xs().child(t("apps.incomplete"))))
          .when(self.scanning, |view| view.child(div().text_sm().child(t("apps.scanning"))))
          .when_some(self.error.clone(), |view, error| view.child(div().text_sm().text_color(theme.danger).child(error)))
          .when_some(self.message.clone(), |view, message| view.child(div().text_sm().child(message))),
      )
      .child(div().id("applications-scroll").flex_1().min_h_0().overflow_y_scroll().child(list))
      .into_any_element()
  }
}
