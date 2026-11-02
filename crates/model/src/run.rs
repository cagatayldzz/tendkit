//! Planning and execution: preparing a plan, running it, and handling engine events.

use super::*;

impl AppModel {
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
    self.scan_complete = false;
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
            model.scan_complete = true;
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

  pub(super) fn receive(&mut self, event: EngineEvent, operation: Operation) {
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
        if let Some(entries) = data.path_entries {
          self.scan_path_entries = Some(entries);
        }
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
pub(crate) fn strip_ansi(text: &str) -> String {
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
