use super::{
  application_usage::{AppUsage, Sampler},
  disk::size,
  platform,
};
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
use std::{
  collections::{HashMap, HashSet},
  path::PathBuf,
  time::Duration,
};
use tendkit_i18n::{t, tf};
use tendkit_model::AppModel;
use tendkit_storage::applications::{self, ApplicationInventory, ApplicationOrigin, InstalledApplication, RemovalBlock};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Filter {
  All,
  Removable,
  Protected,
  Running,
}

pub(super) struct ApplicationsPage {
  model: Entity<AppModel>,
  inventory: ApplicationInventory,
  running: HashSet<PathBuf>,
  usage: HashMap<PathBuf, AppUsage>,
  usage_error: Option<String>,
  usage_task: Option<Task<()>>,
  search: Entity<InputState>,
  filter: Filter,
  loaded: bool,
  scanning: bool,
  removing: Option<PathBuf>,
  visible: usize,
  sort_size: bool,
  expanded: Option<PathBuf>,
  scope_expanded: bool,
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
      usage: HashMap::new(),
      usage_error: None,
      usage_task: None,
      search,
      filter: Filter::Removable,
      loaded: false,
      scanning: false,
      removing: None,
      visible: 100,
      sort_size: false,
      expanded: None,
      scope_expanded: false,
      error: None,
      message: None,
      _subscriptions: vec![subscription, model_subscription],
    }
  }

  pub(super) fn set_active(&mut self, active: bool, cx: &mut Context<Self>) {
    if !active {
      self.usage_task = None;
      return;
    }
    if !self.loaded {
      self.scan(cx);
    }
    if self.usage_task.is_some() || !cfg!(target_os = "macos") {
      return;
    }
    self.usage.clear();
    let executor = cx.background_executor().clone();
    self.usage_task = Some(cx.spawn(async move |this, cx| {
      let mut sampler = Sampler::default();
      loop {
        let Ok(paths) = this.update(cx, |this, _| this.inventory.applications.iter().map(|app| app.path.clone()).collect::<Vec<_>>()) else { break };
        let task = executor.spawn(async move {
          let result = sampler.sample(&paths);
          (sampler, result, platform::running_application_paths())
        });
        let (next, result, mut running) = task.await;
        sampler = next;
        if this
          .update(cx, |this, cx| {
            match result {
              Ok(usage) => {
                running.extend(usage.keys().cloned());
                this.usage = usage;
                this.usage_error = None;
              }
              Err(error) => {
                this.usage.clear();
                this.usage_error = Some(error);
              }
            }
            this.running = running;
            cx.notify();
          })
          .is_err()
        {
          break;
        }
        executor.timer(Duration::from_secs(2)).await;
      }
    }));
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
            this.expanded = None;
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
    let removable = self.inventory.applications.len() - protected;
    let mut apps: Vec<_> = self
      .inventory
      .applications
      .iter()
      .filter(|app| {
        let matches_filter = match self.filter {
          Filter::All => true,
          Filter::Removable => app.blocked.is_none(),
          Filter::Protected => app.blocked.is_some(),
          Filter::Running => self.running.contains(&app.path),
        };
        matches_filter
          && (app.name.to_lowercase().contains(&query)
            || app.path.to_string_lossy().to_lowercase().contains(&query)
            || app.bundle_id.as_ref().is_some_and(|id| id.to_lowercase().contains(&query)))
      })
      .collect();
    if self.sort_size {
      apps.sort_by(|left, right| right.bytes.cmp(&left.bytes).then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase())));
    }
    let shown = apps.len();
    let mut filters = h_flex().gap_1().flex_wrap();
    for (filter, key, count) in [
      (Filter::Removable, "apps.removable", removable),
      (Filter::All, "apps.all", self.inventory.applications.len()),
      (Filter::Protected, "apps.protected", protected),
      (Filter::Running, "apps.running", self.inventory.applications.iter().filter(|app| self.running.contains(&app.path)).count()),
    ] {
      filters = filters.child(
        Button::new(SharedString::from(key))
          .ghost()
          .small()
          .selected(self.filter == filter)
          .label(tf("apps.filter_count", &[("name", &t(key)), ("count", &count.to_string())]))
          .on_click(cx.listener(move |this, _, _, cx| {
            this.filter = filter;
            this.visible = 100;
            cx.notify();
          })),
      );
    }
    let mut list = v_flex().w_full();
    for (index, app) in apps.iter().take(self.visible).enumerate() {
      let is_running = self.running.contains(&app.path);
      let usage = self.usage.get(&app.path);
      let cpu = usage.and_then(|usage| usage.cpu).map(|cpu| format!("{cpu:.1}%")).unwrap_or_else(|| "—".into());
      let memory = usage.filter(|usage| usage.memory > 0).map(|usage| size(usage.memory)).unwrap_or_else(|| "—".into());
      let removing = self.removing.as_ref() == Some(&app.path);
      let expanded = self.expanded.as_ref() == Some(&app.path);
      let toggle_path = app.path.clone();
      let context_path = app.path.clone();
      let reveal = app.path.clone();
      let open = app.path.clone();
      let copy = app.path.clone();
      let app_to_remove = (*app).clone();
      let reason = app.blocked.map(block_key);
      let origin = t(match app.origin {
        ApplicationOrigin::System => "apps.origin_system",
        ApplicationOrigin::Shared => "apps.origin_shared",
        ApplicationOrigin::User => "apps.origin_user",
      });
      let mut details = v_flex()
        .gap_3()
        .mx_5()
        .mb_3()
        .p_3()
        .rounded_md()
        .bg(theme.foreground.opacity(0.035))
        .child(div().text_xs().child(app.path.to_string_lossy().into_owned()))
        .when_some(app.bundle_id.clone(), |column, id| column.child(div().text_xs().text_color(theme.muted_foreground).child(id)));
      if let Some(reason) = reason {
        details = details.child(div().text_sm().child(t(reason)));
      } else if is_running {
        details = details.child(div().text_sm().child(t("apps.running_reason")));
      }
      details = details.child(
        h_flex()
          .gap_2()
          .flex_wrap()
          .child(
            Button::new(SharedString::from(format!("app-open-{index}")))
              .ghost()
              .small()
              .icon(Lucide::AppWindow)
              .label(t("apps.open_app"))
              .on_click(move |_, _, cx| cx.open_with_system(&open)),
          )
          .child(
            Button::new(SharedString::from(format!("app-reveal-{index}")))
              .ghost()
              .small()
              .icon(Lucide::Folder)
              .label(t("disk.reveal"))
              .on_click(move |_, _, cx| cx.reveal_path(&reveal)),
          )
          .child(
            Button::new(SharedString::from(format!("app-copy-{index}")))
              .ghost()
              .small()
              .icon(Lucide::Copy)
              .label(t("disk.copy_path"))
              .on_click(move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(copy.to_string_lossy().into_owned()))),
          )
          .when(reason.is_none(), |row| {
            row.child(
              Button::new(SharedString::from(format!("app-remove-detail-{index}")))
                .ghost()
                .small()
                .icon(Lucide::Trash)
                .label(t(if removing { "apps.moving" } else { "apps.remove" }))
                .disabled(busy || is_running || self.model.read(cx).busy())
                .on_click(cx.listener(move |this, _, window, cx| this.confirm_removal(app_to_remove.clone(), window, cx))),
            )
          })
          .when(app.blocked == Some(RemovalBlock::System), |row| {
            row.child(
              Button::new(SharedString::from(format!("app-help-{index}")))
                .ghost()
                .small()
                .label(t("apps.help"))
                .on_click(|_, _, cx| cx.open_url("https://support.apple.com/102610")),
            )
          }),
      );
      let app_to_remove = (*app).clone();
      let row = h_flex()
        .id(SharedString::from(format!("application-{index}")))
        .w_full()
        .min_h(px(64.))
        .gap_3()
        .px_5()
        .py_2()
        .cursor_pointer()
        .hover(|style| style.bg(theme.foreground.opacity(0.035)))
        .on_click(cx.listener(move |this, _, _, cx| {
          this.expanded = if this.expanded.as_ref() == Some(&toggle_path) { None } else { Some(toggle_path.clone()) };
          cx.notify();
        }))
        .child(Icon::new(Lucide::AppWindow).size(px(24.)).text_color(if app.blocked.is_some() { theme.muted_foreground } else { theme.blue }))
        .child(
          v_flex()
            .flex_1()
            .min_w_0()
            .gap_1()
            .child(
              h_flex()
                .gap_2()
                .child(div().text_sm().font_weight(FontWeight::MEDIUM).truncate().child(app.name.clone()))
                .when_some(app.version.clone(), |row, version| row.child(div().text_xs().text_color(theme.muted_foreground).child(version))),
            )
            .child(div().text_xs().text_color(theme.muted_foreground).child(origin)),
        )
        .when(app.blocked.is_some(), |row| row.child(Tag::secondary().small().child(t("apps.protected"))))
        .when(is_running, |row| row.child(Tag::info().small().child(t("apps.running"))))
        .child(div().w(px(72.)).text_right().text_sm().child(cpu))
        .child(div().w(px(94.)).text_right().text_sm().child(memory))
        .child(div().w(px(90.)).text_right().text_sm().child(app.bytes.map(size).unwrap_or_else(|| "—".into())))
        .child(
          Button::new(SharedString::from(format!("application-action-{index}")))
            .ghost()
            .small()
            .icon(if reason.is_some() { Lucide::ShieldCheck } else { Lucide::Trash })
            .tooltip(t(reason.unwrap_or(if is_running { "apps.running_reason" } else { "apps.remove_tip" })))
            .disabled(reason.is_none() && (busy || is_running || self.model.read(cx).busy()))
            .on_click(cx.listener(move |this, _, window, cx| {
              if app_to_remove.blocked.is_some() {
                this.expanded = Some(app_to_remove.path.clone());
                cx.notify();
              } else {
                this.confirm_removal(app_to_remove.clone(), window, cx);
              }
            })),
        )
        .child(Icon::new(if expanded { IconName::ChevronDown } else { IconName::ChevronRight }).small())
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
        });
      list = list.child(v_flex().w_full().border_b_1().border_color(theme.border).child(row).when(expanded, |column| column.child(details)));
    }
    if !self.scanning && apps.is_empty() {
      list =
        list.child(v_flex().p_6().gap_2().items_center().text_color(theme.muted_foreground).child(t("apps.empty"))).when(!query.is_empty(), |list| {
          list.child(div().flex().justify_center().pb_4().child(
            Button::new("apps-clear-search").ghost().small().label(t("apps.clear_search")).on_click(cx.listener(|this, _, window, cx| {
              this.search.update(cx, |search, cx| search.set_value("", window, cx));
            })),
          ))
        });
    }
    if !self.scanning && apps.is_empty() && self.filter != Filter::All {
      list = list.child(div().flex().justify_center().pb_4().child(
        Button::new("apps-search-all").ghost().small().label(t("apps.search_all")).on_click(cx.listener(|this, _, _, cx| {
          this.filter = Filter::All;
          cx.notify();
        })),
      ));
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
          .px_5()
          .py_3()
          .border_b_1()
          .border_color(theme.border)
          .child(
            h_flex()
              .gap_2()
              .child(
                div()
                  .flex_1()
                  .min_w_0()
                  .child(Input::new(&self.search).with_size(component::Size::Medium).prefix(Icon::new(IconName::Search).small()).cleanable(true)),
              )
              .child(
                Button::new("applications-sort")
                  .ghost()
                  .small()
                  .icon(if self.sort_size { Lucide::ArrowDownWideNarrow } else { Lucide::ArrowDownAZ })
                  .tooltip(t(if self.sort_size { "disk.sort_size" } else { "disk.sort_name" }))
                  .on_click(cx.listener(|this, _, _, cx| {
                    this.sort_size = !this.sort_size;
                    this.visible = 100;
                    cx.notify();
                  })),
              )
              .child(
                Button::new("applications-refresh")
                  .ghost()
                  .small()
                  .icon(IconName::RefreshCw)
                  .tooltip(t("apps.refresh"))
                  .disabled(busy)
                  .on_click(cx.listener(|this, _, _, cx| this.scan(cx))),
              )
              .child(
                Button::new("applications-info")
                  .ghost()
                  .small()
                  .icon(IconName::Info)
                  .selected(self.scope_expanded)
                  .tooltip(t("apps.about"))
                  .on_click(cx.listener(|this, _, _, cx| {
                    this.scope_expanded = !this.scope_expanded;
                    cx.notify();
                  })),
              ),
          )
          .child(
            h_flex()
              .gap_2()
              .flex_wrap()
              .child(filters)
              .child(div().flex_1())
              .child(div().text_xs().text_color(theme.muted_foreground).child(tf("apps.visible_count", &[("count", &shown.to_string())]))),
          )
          .when(self.scope_expanded, |view| {
            view.child(
              v_flex()
                .gap_2()
                .p_3()
                .rounded_md()
                .bg(theme.foreground.opacity(0.035))
                .child(div().text_xs().text_color(theme.muted_foreground).child(t("apps.scope")))
                .child(div().text_xs().child(t("apps.protection_note")))
                .child(div().text_xs().child(t("apps.usage_note")))
                .child(
                  Button::new("applications-help")
                    .ghost()
                    .small()
                    .label(t("apps.help"))
                    .on_click(|_, _, cx| cx.open_url("https://support.apple.com/102610")),
                ),
            )
          })
          .when(self.inventory.incomplete, |view| view.child(div().text_xs().child(t("apps.incomplete"))))
          .when(self.scanning, |view| view.child(div().text_sm().child(t("apps.scanning"))))
          .when_some(self.error.clone(), |view, error| view.child(div().text_sm().text_color(theme.danger).child(error)))
          .when_some(self.message.clone(), |view, message| view.child(div().text_sm().child(message))),
      )
      .when_some(self.usage_error.clone(), |view, error| {
        view.child(div().px_5().py_2().text_xs().child(tf("apps.usage_unavailable", &[("error", &error)])))
      })
      .child(
        h_flex()
          .px_5()
          .py_2()
          .gap_3()
          .text_xs()
          .text_color(theme.muted_foreground)
          .child(div().flex_1().child(t("apps.application")))
          .child(div().w(px(72.)).text_right().child(t("apps.cpu")))
          .child(div().w(px(94.)).text_right().child(t("apps.ram")))
          .child(div().w(px(90.)).text_right().child(t("disk.size")))
          .child(div().w(px(60.))),
      )
      .child(div().id("applications-scroll").flex_1().min_h_0().overflow_y_scroll().child(list))
      .into_any_element()
  }
}
