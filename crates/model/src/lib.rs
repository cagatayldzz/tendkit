//! Application state: the tool catalog, scanning, planning, and execution, driven
//! by engine events and observed by the interface.

mod app_update;

pub use app_update::{AppUpdate, APP_VERSION};

use futures::{channel::mpsc, StreamExt};
use gpui_kit::*;
use std::{
  collections::{BTreeSet, HashMap, HashSet},
  path::PathBuf,
  sync::Arc,
  time::{Duration, SystemTime},
};
use tendkit_client::{EngineConnection, Request};
use tendkit_i18n::{system_language, Language};
use tendkit_protocol::{EngineEvent, EngineFailure, EventType, InstallPlan, Installation, RunStatus, StepStatus, TOOL_IDS};
use tendkit_storage::{AppSettings, LatestVersionCache};
use tendkit_updater::{Release, Staged};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolCategory {
  PackageManager,
  JavaScript,
  Python,
  Rust,
  Ai,
}

impl ToolCategory {
  pub const ALL: [Self; 5] = [Self::PackageManager, Self::JavaScript, Self::Python, Self::Rust, Self::Ai];

  fn key(self) -> &'static str {
    match self {
      Self::PackageManager => "category.package_manager",
      Self::JavaScript => "category.javascript",
      Self::Python => "category.python",
      Self::Rust => "category.rust",
      Self::Ai => "category.ai",
    }
  }

  pub fn title(self) -> String {
    tendkit_i18n::t(self.key())
  }

  pub fn description(self) -> String {
    tendkit_i18n::t(&format!("{}.description", self.key()))
  }
}

#[derive(Clone, Debug)]
pub struct ToolRow {
  pub id: &'static str,
  pub name: &'static str,
  pub installations: Vec<Installation>,
  /// Reported by the engine, which owns the installation source rules.
  pub manageable: bool,
  pub scanned: bool,
}

impl ToolRow {
  /// Display metadata only; the tool list itself comes from `TOOL_IDS`.
  fn new(id: &'static str) -> Self {
    let name = match id {
      "homebrew" => "Homebrew",
      "node" => "Node.js",
      "npm" => "npm",
      "bun" => "Bun",
      "python" => "Python",
      "rust" => "Rust",
      "codex" => "Codex",
      "claude" => "Claude Code",
      "opencommit" => "OpenCommit",
      _ => id,
    };
    Self { id, name, installations: Vec::new(), manageable: false, scanned: false }
  }

  /// A short description in the active language.
  pub fn subtitle(&self) -> String {
    tendkit_i18n::t(&format!("tool.{}.subtitle", self.id))
  }

  pub fn category(&self) -> ToolCategory {
    match self.id {
      "homebrew" => ToolCategory::PackageManager,
      "node" | "npm" | "bun" => ToolCategory::JavaScript,
      "python" => ToolCategory::Python,
      "rust" => ToolCategory::Rust,
      _ => ToolCategory::Ai,
    }
  }

  pub fn primary(&self) -> Option<&Installation> {
    self.installations.first()
  }

  pub fn installed_version(&self) -> Option<&str> {
    self.primary().and_then(|i| i.version.as_deref())
  }
}

/// Whether an installed tool has a newer release, from the latest-version lookups.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateStatus {
  /// The latest version has not been looked up yet.
  Unknown,
  Checking,
  UpToDate,
  Available {
    installed: String,
    latest: String,
  },
  /// The lookup failed, for example when offline or rate limited.
  Unavailable,
}

#[derive(Clone, Debug)]
pub struct StepOutcome {
  pub status: StepStatus,
  pub before: Option<String>,
  pub after: Option<String>,
  pub message: Option<String>,
}

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
  pub tools: Vec<ToolRow>,
  pub is_scanning: bool,
  pub error: Option<String>,
  pub last_scan: Option<SystemTime>,
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
      tools: TOOL_IDS.iter().map(|id| ToolRow::new(id)).collect(),
      is_scanning: false,
      error: None,
      last_scan: None,
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
    } else if let Some(latest) = self.latest_versions.get(tool.id) {
      if tendkit_version::is_newer(latest, installed) {
        UpdateStatus::Available { installed: installed.into(), latest: latest.clone() }
      } else {
        UpdateStatus::UpToDate
      }
    } else if self.latest_version_errors.contains_key(tool.id) {
      UpdateStatus::Unavailable
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
    !self.checking_latest.is_empty()
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
      if let Some(cached) = self.latest_cache.fresh(&key, SystemTime::now()) {
        self.latest_versions.insert(id.into(), cached.version.clone());
        cx.notify();
        return;
      }
      if let Some(date) = self.latest_failed_at.get(&key) {
        if SystemTime::now().duration_since(*date).unwrap_or_default() < Duration::from_secs(600) {
          return;
        }
      }
    }
    let generation = self.latest_generation;
    self.checking_latest.insert(id.into());
    self.latest_version_errors.remove(id);
    cx.notify();
    let request = Request::new("latest").selected(vec![id.into()]).configure_shell(false);
    let (tool, event_key) = (id.to_string(), key.clone());
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
          model.latest_cache.store(&version, &event_key, SystemTime::now());
          model.latest_versions.insert(tool.clone(), version);
          model.latest_failed_at.remove(&event_key);
        }
      },
      move |model, result, _| {
        if generation != model.latest_generation {
          return;
        }
        model.checking_latest.remove(&done_tool);
        if model.checking_latest.is_empty() {
          model.last_update_check = Some(SystemTime::now());
        }
        let Err(error) = result else { return };
        model.latest_failed_at.insert(key.clone(), SystemTime::now());
        // Prefer an expired cached version over no answer, e.g. when rate limited.
        if let Some(stale) = model.latest_cache.entry(&key).cloned() {
          model.latest_versions.insert(done_tool.clone(), stale.version.clone());
          model.log.push(format!("Latest {done_tool} version check failed; showing cached {}: {error}", stale.version));
        } else {
          model.latest_version_errors.insert(done_tool, failure_text(&error));
        }
      },
    );
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
    self.scan(cx);
  }

  pub fn scan(&mut self, cx: &mut Context<Self>) {
    if self.busy() {
      return;
    }
    self.is_scanning = true;
    self.error = None;
    self.run_status = None;
    self.results.clear();
    self.plan = None;
    self.log = vec!["Local tool scan started.".into()];
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
      |model, result, cx| {
        model.is_scanning = false;
        match result {
          Ok(()) => {
            model.last_scan = Some(SystemTime::now());
            model.check_all_latest(false, cx);
          }
          Err(error) => {
            model.error = Some(failure_text(&error));
            model.log.push(format!("Error: {error}"));
          }
        }
      },
    );
  }

  pub fn prepare(&mut self, ids: Option<BTreeSet<String>>, cx: &mut Context<Self>) {
    if self.busy() {
      return;
    }
    let selected = ids.unwrap_or_else(|| self.selected.clone());
    if selected.is_empty() {
      return;
    }
    self.error = None;
    self.plan = None;
    self.run_status = None;
    self.results.clear();
    self.is_planning = true;
    self.log = vec!["Preparing the operation plan.".into()];
    let connection = Arc::new(EngineConnection::new());
    self.connection = Some(connection.clone());
    cx.notify();
    let request = Request::new("plan").selected(selected.into_iter().collect()).configure_shell(self.configure_shell);
    Self::run_engine(
      connection,
      request,
      cx,
      |model, event, _| model.receive(event, Operation::Plan),
      |model, result, _| {
        model.is_planning = false;
        model.connection = None;
        match result {
          Ok(()) => model.show_plan = model.plan.is_some(),
          Err(error) => model.record(&error),
        }
      },
    );
  }

  pub fn dismiss_plan(&mut self, cx: &mut Context<Self>) {
    self.show_plan = false;
    cx.notify();
  }

  pub fn execute(&mut self, cx: &mut Context<Self>) {
    if self.busy() {
      return;
    }
    let Some(plan) = self.plan.clone() else { return };
    self.show_plan = false;
    self.is_executing = true;
    self.is_cancelling = false;
    self.error = None;
    self.results.clear();
    self.run_status = None;
    self.active_step = None;
    self.log = vec!["Running the approved operation plan.".into()];
    let connection = Arc::new(EngineConnection::new());
    self.connection = Some(connection.clone());
    cx.notify();
    Self::run_engine(
      connection,
      Request::new("execute").plan(plan),
      cx,
      |model, event, cx| {
        let requests_installer = event.kind == EventType::ExternalActionRequired;
        model.receive(event, Operation::Execute);
        if requests_installer && model.installer_path.is_some() {
          model.open_installer(cx);
        }
      },
      |model, result, cx| {
        match result {
          Ok(()) => {
            model.last_scan = Some(SystemTime::now());
            if model.run_status == Some(RunStatus::Succeeded) {
              model.selected.clear();
            }
          }
          Err(error) => {
            model.run_status = Some(RunStatus::Failed);
            model.record(&error);
          }
        }
        model.is_executing = false;
        model.is_cancelling = false;
        model.installer_path = None;
        model.active_step = None;
        model.connection = None;
        if model.pending_termination {
          cx.quit();
        }
      },
    );
  }

  /// Hides the finished operation summary.
  pub fn dismiss_run_status(&mut self, cx: &mut Context<Self>) {
    if self.is_executing {
      return;
    }
    self.run_status = None;
    self.results.clear();
    self.plan = None;
    cx.notify();
  }

  pub fn cancel(&mut self, cx: &mut Context<Self>) {
    if !self.is_executing || self.is_cancelling {
      return;
    }
    self.is_cancelling = true;
    if let Some(connection) = &self.connection {
      connection.cancel();
    }
    if self.installer_path.is_some() {
      self.append_log("Close the open macOS Installer window separately. Further steps have been stopped.");
    }
    cx.notify();
  }

  pub fn open_installer(&mut self, cx: &mut Context<Self>) {
    let Some(path) = self.installer_path.clone() else { return };
    let opened = std::process::Command::new("/usr/bin/open").arg(&path).status().map(|s| s.success()).unwrap_or(false);
    if !opened {
      self.error = Some(tendkit_i18n::t("err.installer_open"));
    }
    cx.notify();
  }

  pub fn continue_after_installer(&mut self, cx: &mut Context<Self>) {
    if self.installer_path.is_none() || self.is_cancelling {
      return;
    }
    self.installer_path = None;
    if let Some(connection) = &self.connection {
      connection.acknowledge_installer();
    }
    cx.notify();
  }

  fn record(&mut self, error: &EngineFailure) {
    self.error = Some(failure_text(error));
    self.append_log(&format!("Error: {error}"));
  }

  fn append_log(&mut self, message: &str) {
    let clean: String = strip_ansi(message).chars().take(8192).collect();
    self.log.push(clean);
    if self.log.len() > 1000 {
      self.log.drain(..self.log.len() - 1000);
    }
  }

  fn receive(&mut self, event: EngineEvent, operation: Operation) {
    let data = event.data;
    match event.kind {
      EventType::Hello => self.engine_version = data.engine_version,
      EventType::Log => self.append_log(data.message.as_deref().unwrap_or("")),
      EventType::PlanReady => self.plan = data.plan,
      EventType::StepStarted => self.active_step = data.id,
      EventType::StepFinished => {
        let Some(id) = data.id.clone() else { return };
        let outcome = StepOutcome {
          status: data.step_status().unwrap_or(StepStatus::Failed),
          before: data.before.and_then(|i| i.version),
          after: data.after.and_then(|i| i.version),
          message: data.message.clone(),
        };
        let suffix = data.message.map(|m| format!(" · {m}")).unwrap_or_default();
        self.append_log(&format!("{id}: {}{suffix}", outcome.status.label()));
        self.results.insert(id, outcome);
        self.active_step = None;
      }
      EventType::ExternalActionRequired => {
        let Some(path) = data.path else { return };
        match verified_installer_package(&path) {
          Some(path) => {
            self.installer_path = Some(path);
            self.append_log(data.message.as_deref().unwrap_or("Waiting for macOS Installer."));
          }
          None => {
            self.error = Some(tendkit_i18n::t("err.installer_path"));
            if self.is_executing && !self.is_cancelling {
              self.is_cancelling = true;
              if let Some(connection) = &self.connection {
                connection.cancel();
              }
            }
          }
        }
      }
      EventType::ToolDetected => {
        let Some(index) = self.tools.iter().position(|t| Some(t.id) == data.id.as_deref()) else { return };
        let tool = &mut self.tools[index];
        tool.installations = data.installations.unwrap_or_default();
        tool.manageable = data.manageable.unwrap_or(false);
        tool.scanned = true;
        let tool = tool.clone();
        let version = match tool.primary() {
          None => "not found",
          Some(installation) => installation.version.as_deref().unwrap_or("version unavailable"),
        };
        self.append_log(&format!("{}: {version}", tool.name));
        for installation in &tool.installations {
          self.append_log(&format!("  {} · {}", installation.path, installation.source));
          if let Some(error) = &installation.error {
            self.append_log(&format!("  {error}"));
          }
        }
      }
      EventType::RunFinished => {
        if operation == Operation::Execute {
          self.run_status = data.run_status();
        }
        let message = match operation {
          Operation::Scan => "Scan completed.".to_string(),
          Operation::Plan => "Operation plan ready.".to_string(),
          Operation::Execute => format!("Operation completed: {}", data.status.unwrap_or_default()),
        };
        self.append_log(&message);
      }
      EventType::LatestVersion | EventType::Error => {}
    }
  }
}

/// Only a `Homebrew.pkg` inside one of the engine's `tendkit-*` temporary
/// directories may be opened.
fn verified_installer_package(path: &str) -> Option<PathBuf> {
  let resolved = std::fs::canonicalize(path).ok()?;
  let temporary = std::fs::canonicalize(std::env::temp_dir()).ok()?;
  let parent = resolved.parent()?;
  let valid =
    resolved.file_name()? == "Homebrew.pkg" && parent.parent()? == temporary && parent.file_name()?.to_string_lossy().starts_with("tendkit-");
  valid.then_some(resolved)
}

/// Removes ANSI CSI sequences (`ESC [ params intermediates final`).
fn strip_ansi(text: &str) -> String {
  let mut output = String::with_capacity(text.len());
  let mut chars = text.chars().peekable();
  while let Some(c) = chars.next() {
    if c == '\u{1b}' && chars.peek() == Some(&'[') {
      chars.next();
      while chars.next_if(|c| ('0'..='?').contains(c)).is_some() {}
      while chars.next_if(|c| (' '..='/').contains(c)).is_some() {}
      chars.next_if(|c| ('@'..='~').contains(c));
    } else {
      output.push(c);
    }
  }
  output
}

#[cfg(test)]
mod tests;
