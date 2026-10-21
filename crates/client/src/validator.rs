use std::collections::HashSet;
use tendkit_protocol::{EngineEvent, EngineFailure, EventType, InstallPlan, PROTOCOL_VERSION, TOOL_IDS};

/// Checks that engine events arrive in a valid order for one request.
pub struct EventValidator {
  command: String,
  request_id: String,
  plan: Option<InstallPlan>,
  selected: Vec<String>,
  sequence: i64,
  greeted: bool,
  planned: bool,
  received_latest: bool,
  tools: HashSet<String>,
  completed: bool,
}

impl EventValidator {
  pub fn new(command: &str, request_id: &str, plan: Option<InstallPlan>, selected: Vec<String>) -> Self {
    Self {
      command: command.into(),
      request_id: request_id.into(),
      plan,
      selected,
      sequence: 0,
      greeted: false,
      planned: false,
      received_latest: false,
      tools: HashSet::new(),
      completed: false,
    }
  }

  pub fn completed(&self) -> bool {
    self.completed
  }

  pub fn accept(&mut self, event: &EngineEvent) -> Result<(), EngineFailure> {
    use EngineFailure::InvalidProtocol;
    let ensure = |condition: bool| if condition { Ok(()) } else { Err(InvalidProtocol) };
    ensure(
      event.protocol_version == i64::from(PROTOCOL_VERSION)
        && event.request_id == self.request_id
        && event.sequence == self.sequence + 1
        && !self.completed,
    )?;
    self.sequence = event.sequence;
    let data = &event.data;
    let known = |id: &Option<String>| id.as_deref().is_some_and(|id| TOOL_IDS.contains(&id));
    match event.kind {
      EventType::Error => return Err(EngineFailure::Failed(data.message.clone().unwrap_or_else(|| "Engine error".into()))),
      EventType::Hello => {
        ensure(!self.greeted && self.sequence == 1)?;
        self.greeted = true;
      }
      EventType::Log => ensure(self.greeted)?,
      EventType::PlanReady => {
        ensure(self.greeted && self.command == "plan" && !self.planned && data.plan.is_some())?;
        self.planned = true;
      }
      EventType::LatestVersion => {
        ensure(
          self.greeted
            && self.command == "latest"
            && self.selected.len() == 1
            && !self.received_latest
            && known(&data.id)
            && data.id.as_deref() == Some(self.selected[0].as_str())
            && data.latest_version.as_deref().is_some_and(|v| !v.is_empty()),
        )?;
        self.received_latest = true;
      }
      EventType::ToolDetected => {
        ensure(self.greeted && matches!(self.command.as_str(), "scan" | "execute") && known(&data.id) && data.installations.is_some())?;
        ensure(self.tools.insert(data.id.clone().unwrap_or_default()))?;
      }
      EventType::StepStarted | EventType::StepFinished | EventType::ExternalActionRequired => {
        let id = data.id.as_deref();
        let in_plan = self
          .plan
          .as_ref()
          .is_some_and(|plan| plan.steps.iter().any(|s| Some(s.id.as_str()) == id) || (id == Some("shell") && plan.configure_shell));
        ensure(self.greeted && self.command == "execute" && in_plan)?;
        if event.kind == EventType::StepFinished {
          ensure(data.step_status().is_some())?;
        }
      }
      EventType::RunFinished => {
        let done = match self.command.as_str() {
          "plan" => self.planned,
          "latest" => self.received_latest,
          _ => self.tools.len() == TOOL_IDS.len(),
        };
        if !(self.greeted && data.run_status().is_some() && done) {
          return Err(EngineFailure::Incomplete);
        }
        self.completed = true;
      }
    }
    Ok(())
  }
}
