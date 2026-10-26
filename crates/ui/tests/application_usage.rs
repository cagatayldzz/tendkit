use super::*;
use std::time::Duration;

fn process(pid: u32, parent: u32, executable: &str, cpu_seconds: f64, memory: u64) -> Process {
  Process { pid, parent, executable: executable.into(), started: "Mon Oct 26 20:00:00 2026".into(), cpu_seconds, memory }
}

#[test]
fn parses_macos_process_rows_with_spaces_and_converts_rss_to_bytes() {
  let process = parse_process("  42  1 Mon Oct 26 20:00:00 2026 1:02.50 2048 /Applications/Example App.app/Contents/MacOS/Example App").unwrap();
  assert_eq!(process.pid, 42);
  assert_eq!(process.cpu_seconds, 62.5);
  assert_eq!(process.memory, 2 * 1024 * 1024);
  assert_eq!(process.executable, PathBuf::from("/Applications/Example App.app/Contents/MacOS/Example App"));
  assert!(parse_process("invalid row").is_none());
  assert!(cpu_seconds("NaN:00").is_none());
  assert_eq!(cpu_seconds("1-02:03:04.50"), Some(93784.5));
}

#[test]
fn sums_helpers_and_children_but_preserves_separately_launched_app_ownership() {
  let mut sampler = Sampler::default();
  let paths = vec![PathBuf::from("/Applications/Example.app"), PathBuf::from("/Applications/Other.app")];
  let now = Instant::now();
  let rows = |offset| {
    vec![
      process(1, 0, "/Applications/Example.app/Contents/MacOS/main", 1. + offset, 100),
      process(2, 1, "/Applications/Example.app/Contents/Helpers/Worker.app/worker", 2. + offset, 200),
      process(3, 2, "/usr/bin/worker", 3. + offset, 300),
      process(4, 1, "/Applications/Other.app/Contents/MacOS/main", 4. + offset, 400),
    ]
  };
  let first = sampler.aggregate(rows(0.), &paths, now);
  assert_eq!(first[&paths[0]].cpu, None);
  let second = sampler.aggregate(rows(1.), &paths, now + Duration::from_secs(2));
  assert_eq!(second[&paths[0]].memory, 600);
  assert_eq!(second[&paths[0]].processes, 3);
  assert_eq!(second[&paths[0]].cpu, Some(150.));
  assert_eq!(second[&paths[1]].cpu, Some(50.));
}

#[test]
fn exited_processes_disappear_and_reused_pids_need_a_new_cpu_sample() {
  let mut sampler = Sampler::default();
  let paths = vec![PathBuf::from("/Applications/Example.app")];
  let now = Instant::now();
  sampler.aggregate(vec![process(1, 0, "/Applications/Example.app/main", 10., 100)], &paths, now);
  let mut replacement = process(1, 0, "/Applications/Example.app/main", 20., 200);
  replacement.started = "Mon Oct 26 20:01:00 2026".into();
  let second = sampler.aggregate(vec![replacement], &paths, now + Duration::from_secs(2));
  assert_eq!(second[&paths[0]].cpu, None);
  assert!(sampler.aggregate(Vec::new(), &paths, now + Duration::from_secs(4)).is_empty());
}

#[test]
fn cyclic_parent_chains_do_not_hang_or_assign_unrelated_processes() {
  let mut sampler = Sampler::default();
  let usage = sampler.aggregate(
    vec![process(1, 2, "/usr/bin/a", 1., 100), process(2, 1, "/usr/bin/b", 1., 100)],
    &[PathBuf::from("/Applications/Example.app")],
    Instant::now(),
  );
  assert!(usage.is_empty());
}
