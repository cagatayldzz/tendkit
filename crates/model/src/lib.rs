//! Application state: the tool catalog, scanning, planning, and execution, driven
//! by engine events and observed by the interface.

mod app_update;
mod disk;
mod doctor;
mod run;
mod tools;
pub use tendkit_storage::disk::{DiskEntry, DiskReport};

pub use app_update::{AppUpdate, ThemeChoice, APP_UPDATES_ENABLED, APP_VERSION};
pub use doctor::{DoctorCheck, DoctorFinding, DoctorInstallation, DoctorReport, DoctorSeverity};
pub use tools::{StepOutcome, ToolCategory, ToolRow, UpdateStatus};

use futures::{channel::mpsc, StreamExt};
use gpui_kit::*;
use std::{
  cell::RefCell,
  collections::{BTreeSet, HashMap, HashSet, VecDeque},
  path::PathBuf,
  rc::Rc,
  sync::Arc,
  time::{Duration, SystemTime},
};
use tendkit_client::{EngineConnection, Request};
use tendkit_i18n::{system_language, Language};
use tendkit_protocol::{EngineEvent, EngineFailure, EventType, InstallPlan, Installation, RunStatus, StepStatus, TOOL_IDS};
use tendkit_storage::{AppSettings, LatestVersionCache};
use tendkit_updater::{Release, Staged};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Operation {
  Scan,
  Plan,
  Execute,
}

#[allow(clippy::large_enum_variant)]
enum Message {
  Event(EngineEvent),
  Done(Result<(), EngineFailure>),
}

pub struct AppModel {
  pub disk: disk::DiskState,
  pub tools: Vec<ToolRow>,
  pub is_scanning: bool,
  pub error: Option<String>,
  pub last_scan: Option<SystemTime>,
  pub scan_path_entries: Option<Vec<String>>,
  pub scan_complete: bool,
  pub log: Vec<String>,
  pub engine_version: Option<String>,
  pub latest_versions: HashMap<String, String>,
  pub latest_version_errors: HashMap<String, String>,
  pub checking_latest: HashSet<String>,
  /// When the last full round of latest-version lookups finished.
  pub last_update_check: Option<SystemTime>,
  pub selected: BTreeSet<String>,
  pub configure_shell: bool,
  pub is_planning: bool,
  pub is_executing: bool,
  pub is_cancelling: bool,
  pub show_plan: bool,
  pub plan: Option<InstallPlan>,
  pub results: HashMap<String, StepOutcome>,
  pub active_step: Option<String>,
  pub run_status: Option<RunStatus>,
  pub installer_path: Option<PathBuf>,
  pub pending_termination: bool,
  pub settings: AppSettings,
  pub app_update: AppUpdate,
  pub app_update_checked: Option<SystemTime>,
  last_release: Option<Release>,
  staged: Option<Staged>,
  latest_cache: LatestVersionCache,
  /// Failed lookups back off so a rate-limited endpoint is not retried on every tool selection.
  latest_failed_at: HashMap<String, SystemTime>,
  latest_generation: u64,
  pending_latest: VecDeque<(String, bool)>,
  connection: Option<Arc<EngineConnection>>,
}

/// An engine failure in the active language. Messages from the engine itself stay as reported.
fn failure_text(error: &EngineFailure) -> String {
  match error {
    EngineFailure::Unavailable => tendkit_i18n::t("err.engine_unavailable"),
    EngineFailure::InvalidProtocol => tendkit_i18n::t("err.engine_protocol"),
    EngineFailure::Incomplete => tendkit_i18n::t("err.engine_incomplete"),
    EngineFailure::OversizedMessage => tendkit_i18n::t("err.engine_oversized"),
    EngineFailure::Failed(message) => message.clone(),
  }
}

/// Switches both TendKit's texts and the built-in texts of the interface kit.
pub(crate) fn apply_language(language: Language) {
  tendkit_i18n::set_language(language);
  gpui_kit::component::set_locale(language.code());
}

/// The language to show: the saved choice, or else the language of this computer
/// (English when the computer's languages are not supported).
fn effective_language(settings: &AppSettings) -> Language {
  settings.language.as_deref().and_then(Language::from_code).unwrap_or_else(system_language)
}

impl Default for AppModel {
  fn default() -> Self {
    Self::new()
  }
}

impl AppModel {
  pub fn new() -> Self {
    let settings = AppSettings::load(&AppSettings::default_path());
    apply_language(effective_language(&settings));
    Self {
      disk: disk::DiskState::default(),
      tools: TOOL_IDS.iter().map(|id| ToolRow::new(id)).collect(),
      is_scanning: false,
      error: None,
      last_scan: None,
      scan_path_entries: None,
      scan_complete: false,
      log: Vec::new(),
      engine_version: None,
      latest_versions: HashMap::new(),
      latest_version_errors: HashMap::new(),
      checking_latest: HashSet::new(),
      last_update_check: None,
      selected: BTreeSet::new(),
      configure_shell: settings.configure_shell,
      is_planning: false,
      is_executing: false,
      is_cancelling: false,
      show_plan: false,
      plan: None,
      results: HashMap::new(),
      active_step: None,
      run_status: None,
      installer_path: None,
      pending_termination: false,
      settings,
      app_update: AppUpdate::Idle,
      app_update_checked: None,
      last_release: None,
      staged: None,
      latest_cache: LatestVersionCache::load(LatestVersionCache::default_path()),
      latest_failed_at: HashMap::new(),
      latest_generation: 0,
      pending_latest: VecDeque::new(),
      connection: None,
    }
  }

  pub fn busy(&self) -> bool {
    self.is_scanning || self.is_planning || self.is_executing
  }

  pub fn progress_total(&self) -> usize {
    self.plan.as_ref().map(|p| p.steps.len() + usize::from(p.configure_shell)).unwrap_or(0)
  }

  pub fn installed_count(&self) -> usize {
    self.tools.iter().filter(|t| t.installed_version().is_some()).count()
  }

  pub fn scanned_count(&self) -> usize {
    self.tools.iter().filter(|t| t.scanned).count()
  }

  pub fn tool(&self, id: &str) -> Option<&ToolRow> {
    self.tools.iter().find(|t| t.id == id)
  }

  pub fn can_manage(&self, tool: &ToolRow) -> bool {
    tool.scanned && tool.manageable
  }

  /// The update state of an installed tool; `None` when it is not installed.
  pub fn update_status(&self, tool: &ToolRow) -> Option<UpdateStatus> {
    let installed = tool.installed_version()?;
    Some(if self.checking_latest.contains(tool.id) {
      UpdateStatus::Checking
    } else if self.latest_is_stale(tool.id) {
      UpdateStatus::Unavailable
    } else if let Some(latest) = self.latest_versions.get(tool.id) {
      if tendkit_version::is_newer(latest, installed) {
        UpdateStatus::Available { installed: installed.into(), latest: latest.clone() }
      } else {
        UpdateStatus::UpToDate
      }
    } else {
      UpdateStatus::Unknown
    })
  }

  pub fn has_update(&self, tool: &ToolRow) -> bool {
    matches!(self.update_status(tool), Some(UpdateStatus::Available { .. }))
  }

  /// Whether TendKit can apply the update. A newer release can exist without being
  /// actionable: npm updates with Node.js, and externally managed tools stay with their manager.
  pub fn can_update(&self, tool: &ToolRow) -> bool {
    tool.id != "npm" && self.can_manage(tool) && self.has_update(tool)
  }

  /// Tools with an update that TendKit can apply.
  pub fn updatable_ids(&self) -> BTreeSet<String> {
    self.tools.iter().filter(|t| self.can_update(t)).map(|t| t.id.to_string()).collect()
  }

  pub fn update_count(&self) -> usize {
    self.tools.iter().filter(|t| self.can_update(t)).count()
  }

  pub fn is_checking_updates(&self) -> bool {
    !self.checking_latest.is_empty() || !self.pending_latest.is_empty()
  }

  /// Looks up the latest version of every installed tool.
  pub fn check_all_latest(&mut self, force: bool, cx: &mut Context<Self>) {
    let ids: Vec<_> = self.tools.iter().filter(|t| t.installed_version().is_some()).map(|t| t.id).collect();
    for id in ids {
      self.check_latest_version(id, force, cx);
    }
  }

  /// Plans an update of every tool that has one.
  pub fn update_all(&mut self, cx: &mut Context<Self>) {
    let ids = self.updatable_ids();
    self.prepare(Some(ids), cx);
  }

  pub fn toggle_selected(&mut self, id: &str, selected: bool, cx: &mut Context<Self>) {
    if selected {
      self.selected.insert(id.into());
    } else {
      self.selected.remove(id);
    }
    cx.notify();
  }

  /// Runs one engine request on a background thread and delivers its events
  /// to `handle` on the foreground.
  fn run_engine(
    connection: Arc<EngineConnection>,
    request: Request,
    cx: &mut Context<Self>,
    handle: impl Fn(&mut Self, EngineEvent, &mut Context<Self>) + 'static,
    done: impl FnOnce(&mut Self, Result<(), EngineFailure>, &mut Context<Self>) + 'static,
  ) {
    let (sender, mut receiver) = mpsc::unbounded();
    std::thread::spawn(move || {
      let events = sender.clone();
      let result = connection.run(&request, None, |event| {
        let _ = events.unbounded_send(Message::Event(event));
      });
      let _ = sender.unbounded_send(Message::Done(result));
    });
    cx.spawn(async move |this, cx| {
      let mut done = Some(done);
      while let Some(message) = receiver.next().await {
        let alive = this.update(cx, |model, cx| {
          match message {
            Message::Event(event) => handle(model, event, cx),
            Message::Done(result) => {
              if let Some(done) = done.take() {
                done(model, result, cx)
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

  pub fn check_latest_version(&mut self, id: &str, force: bool, cx: &mut Context<Self>) {
    if self.is_scanning || self.checking_latest.contains(id) {
      return;
    }
    let key = LatestVersionCache::key(id, self.tool(id).and_then(|t| t.primary()).map(|i| i.source.as_str()));
    if !force {
      if let Some(date) = self.latest_failed_at.get(&key) {
        if SystemTime::now().duration_since(*date).unwrap_or_default() < Duration::from_secs(600) {
          return;
        }
      } else if let Some(cached) = self.latest_cache.fresh(&key, SystemTime::now()) {
        self.latest_versions.insert(id.into(), cached.version.clone());
        self.latest_version_errors.remove(id);
        cx.notify();
        return;
      }
    }
    if self.checking_latest.len() >= 3 {
      if !self.pending_latest.iter().any(|(pending, _)| pending == id) {
        self.pending_latest.push_back((id.into(), force));
      }
      return;
    }
    let generation = self.latest_generation;
    self.checking_latest.insert(id.into());
    self.latest_version_errors.remove(id);
    cx.notify();
    let request = Request::new("latest").selected(vec![id.into()]).configure_shell(false);
    let pending_version = Rc::new(RefCell::new(None));
    let event_version = pending_version.clone();
    let done_tool = id.to_string();
    Self::run_engine(
      Arc::new(EngineConnection::new()),
      request,
      cx,
      move |model, event, _| {
        if event.kind != EventType::LatestVersion || generation != model.latest_generation {
          return;
        }
        if let Some(version) = event.data.latest_version {
          *event_version.borrow_mut() = Some(version);
        }
      },
      move |model, result, cx| {
        if generation != model.latest_generation {
          return;
        }
        model.checking_latest.remove(&done_tool);
        if result.is_ok() {
          if let Some(version) = pending_version.borrow_mut().take() {
            model.latest_cache.store(&version, &key, SystemTime::now());
            model.latest_versions.insert(done_tool.clone(), version);
            model.latest_failed_at.remove(&key);
            model.latest_version_errors.remove(&done_tool);
          }
        }
        if let Err(error) = result {
          model.latest_failed_at.insert(key.clone(), SystemTime::now());
          model.latest_version_errors.insert(done_tool.clone(), failure_text(&error));
          if let Some(stale) = model.latest_cache.entry(&key).cloned() {
            model.latest_versions.insert(done_tool.clone(), stale.version.clone());
            model.log.push(format!("Latest {done_tool} version check failed; showing cached {}: {error}", stale.version));
          }
        }
        while model.checking_latest.len() < 3 {
          let Some((id, force)) = model.pending_latest.pop_front() else { break };
          model.check_latest_version(&id, force, cx);
        }
        if !model.is_checking_updates() {
          model.last_update_check = Some(SystemTime::now());
        }
      },
    );
  }

  pub fn latest_is_stale(&self, id: &str) -> bool {
    self.latest_version_errors.contains_key(id)
      || self.latest_checked_at(id).is_some_and(|at| SystemTime::now().duration_since(at).unwrap_or_default() >= LatestVersionCache::LIFETIME)
  }

  pub fn latest_checked_at(&self, id: &str) -> Option<SystemTime> {
    let key = LatestVersionCache::key(id, self.tool(id).and_then(|t| t.primary()).map(|i| i.source.as_str()));
    self.latest_cache.entry(&key).map(|entry| entry.checked_at)
  }

  pub fn refresh(&mut self, cx: &mut Context<Self>) {
    if self.busy() {
      return;
    }
    self.latest_generation += 1;
    self.latest_versions.clear();
    self.latest_version_errors.clear();
    self.latest_cache.remove_all();
    self.latest_failed_at.clear();
    self.checking_latest.clear();
    self.pending_latest.clear();
    self.scan(cx);
  }

  pub fn scan(&mut self, cx: &mut Context<Self>) {
    self.scan_with_latest(true, cx);
  }

  /// Doctor refreshes only local evidence and never queries release metadata.
  pub fn scan_doctor(&mut self, cx: &mut Context<Self>) {
    self.scan_with_latest(false, cx);
  }

  fn scan_with_latest(&mut self, check_latest: bool, cx: &mut Context<Self>) {
    if self.busy() {
      return;
    }
    self.is_scanning = true;
    self.error = None;
    self.run_status = None;
    self.results.clear();
    self.plan = None;
    self.log = vec!["Local tool scan started.".into()];
    self.scan_path_entries = None;
    self.scan_complete = false;
    for tool in &mut self.tools {
      tool.scanned = false;
      tool.installations.clear();
      tool.manageable = false;
    }
    cx.notify();
    Self::run_engine(
      Arc::new(EngineConnection::new()),
      Request::new("scan"),
      cx,
      |model, event, _| model.receive(event, Operation::Scan),
      move |model, result, cx| {
        model.is_scanning = false;
        match result {
          Ok(()) => {
            model.last_scan = Some(SystemTime::now());
            model.scan_complete = true;
            if check_latest {
              model.check_all_latest(false, cx);
            }
          }
          Err(error) => {
            model.error = Some(failure_text(&error));
            model.log.push(format!("Error: {error}"));
          }
        }
      },
    );
  }
}

#[cfg(test)]
use run::strip_ansi;

#[cfg(test)]
#[path = "../tests/state.rs"]
mod tests;
