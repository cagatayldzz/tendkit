//! Sample cumulative process CPU time and resident memory without reading arguments.

use std::{
  collections::{HashMap, HashSet},
  path::PathBuf,
  process::Command,
  time::Instant,
};

#[derive(Clone, Debug, Default)]
pub(super) struct AppUsage {
  pub cpu: Option<f64>,
  pub memory: u64,
  pub processes: usize,
}

#[derive(Clone, Debug)]
struct Process {
  pid: u32,
  parent: u32,
  started: String,
  cpu_seconds: f64,
  memory: u64,
  executable: PathBuf,
}

#[derive(Default)]
pub(super) struct Sampler {
  previous: HashMap<u32, Process>,
  sampled_at: Option<Instant>,
}

impl Sampler {
  pub fn sample(&mut self, applications: &[PathBuf]) -> Result<HashMap<PathBuf, AppUsage>, String> {
    let output = Command::new("/bin/ps")
      .args(["-ww", "-axo", "pid=,ppid=,lstart=,time=,rss=,comm="])
      .env("LC_ALL", "C")
      .output()
      .map_err(|error| error.to_string())?;
    if !output.status.success() {
      self.previous.clear();
      self.sampled_at = None;
      return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    let processes: Vec<_> = String::from_utf8_lossy(&output.stdout).lines().filter_map(parse_process).collect();
    if processes.is_empty() {
      return Err("No process metrics were available.".into());
    }
    let aliases: Vec<_> = applications.iter().map(|path| (path.clone(), std::fs::canonicalize(path).unwrap_or_else(|_| path.clone()))).collect();
    let canonical: Vec<_> = aliases.iter().map(|(_, path)| path.clone()).collect();
    let usage = self.aggregate(processes, &canonical, Instant::now());
    Ok(aliases.into_iter().filter_map(|(display, canonical)| usage.get(&canonical).cloned().map(|value| (display, value))).collect())
  }

  fn aggregate(&mut self, processes: Vec<Process>, applications: &[PathBuf], now: Instant) -> HashMap<PathBuf, AppUsage> {
    let elapsed = self.sampled_at.map(|previous| now.duration_since(previous).as_secs_f64()).filter(|elapsed| *elapsed > 0.);
    let current: HashMap<_, _> = processes.into_iter().map(|process| (process.pid, process)).collect();
    let mut owners = HashMap::new();
    for process in current.values() {
      if let Some(app) = applications.iter().filter(|app| process.executable.starts_with(app)).max_by_key(|app| app.components().count()) {
        owners.insert(process.pid, app.clone());
      }
    }
    // Follow parent chains for workers outside a bundle. Explicit bundle matches
    // take precedence, so a separately launched application keeps its own usage.
    for process in current.values() {
      if owners.contains_key(&process.pid) {
        continue;
      }
      let mut parent = process.parent;
      let mut visited = HashSet::new();
      while visited.insert(parent) {
        if let Some(owner) = owners.get(&parent).cloned() {
          owners.insert(process.pid, owner);
          break;
        }
        let Some(ancestor) = current.get(&parent) else { break };
        parent = ancestor.parent;
      }
    }
    let mut usage: HashMap<PathBuf, AppUsage> = HashMap::new();
    for process in current.values() {
      let Some(owner) = owners.get(&process.pid) else { continue };
      let value = usage.entry(owner.clone()).or_default();
      value.memory = value.memory.saturating_add(process.memory);
      value.processes += 1;
      if let (Some(elapsed), Some(previous)) = (elapsed, self.previous.get(&process.pid)) {
        if previous.started == process.started && previous.executable == process.executable && process.cpu_seconds >= previous.cpu_seconds {
          let cpu = (process.cpu_seconds - previous.cpu_seconds) / elapsed * 100.;
          value.cpu = Some(value.cpu.unwrap_or(0.) + cpu);
        }
      }
    }
    self.previous = current;
    self.sampled_at = Some(now);
    usage
  }
}

fn field<'a>(rest: &mut &'a str) -> Option<&'a str> {
  *rest = rest.trim_start();
  let end = rest.find(char::is_whitespace)?;
  let value = &rest[..end];
  *rest = &rest[end..];
  Some(value)
}

fn parse_process(line: &str) -> Option<Process> {
  let mut rest = line;
  let pid = field(&mut rest)?.parse().ok()?;
  let parent = field(&mut rest)?.parse().ok()?;
  let mut started = Vec::new();
  for _ in 0..5 {
    started.push(field(&mut rest)?);
  }
  let cpu_seconds = cpu_seconds(field(&mut rest)?)?;
  let memory = field(&mut rest)?.parse::<u64>().ok()?.checked_mul(1024)?;
  let executable = PathBuf::from(rest.trim());
  if executable.as_os_str().is_empty() {
    return None;
  }
  Some(Process { pid, parent, started: started.join(" "), cpu_seconds, memory, executable })
}

fn cpu_seconds(value: &str) -> Option<f64> {
  let (days, time) = value.split_once('-').map(|(days, time)| Some((days.parse::<f64>().ok()?, time))).unwrap_or(Some((0., value)))?;
  let parts: Vec<_> = time.split(':').map(str::parse::<f64>).collect::<Result<_, _>>().ok()?;
  let seconds = match parts.as_slice() {
    [minutes, seconds] => minutes * 60. + seconds,
    [hours, minutes, seconds] => hours * 3600. + minutes * 60. + seconds,
    _ => return None,
  } + days * 86400.;
  (seconds.is_finite() && seconds >= 0.).then_some(seconds)
}

#[cfg(test)]
#[path = "../tests/application_usage.rs"]
mod tests;
