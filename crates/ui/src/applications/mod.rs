use super::{
  application_usage::{AppUsage, Sampler},
  disk::size,
  platform,
};
mod render;

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
