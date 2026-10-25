use std::{
  env, fs,
  path::{Path, PathBuf},
};

pub fn home() -> Result<PathBuf, String> {
  env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from).filter(|p| p.is_absolute()).ok_or("HOME is unavailable".into())
}

fn append_child_bins(parent: PathBuf, dirs: &mut Vec<PathBuf>) {
  if let Ok(entries) = fs::read_dir(parent) {
    let mut children: Vec<_> = entries.flatten().map(|e| e.path().join("bin")).collect();
    children.sort();
    dirs.extend(children);
  }
}

pub fn search_dirs() -> Vec<PathBuf> {
  let mut dirs: Vec<_> = env::split_paths(&env::var_os("PATH").unwrap_or_default()).filter(|p| p.is_absolute()).collect();
  if cfg!(windows) {
    if let Ok(home) = home() {
      for suffix in [".cargo/bin", ".bun/bin", ".local/bin", "AppData/Roaming/npm", "AppData/Local/Microsoft/WinGet/Links"] {
        dirs.push(home.join(suffix));
      }
    }
    for key in ["ProgramFiles", "ProgramFiles(x86)"] {
      if let Some(root) = env::var_os(key) {
        dirs.push(PathBuf::from(root).join("nodejs"));
      }
    }
    return dirs;
  }
  if let Some(home) = env::var_os("HOME") {
    let home = PathBuf::from(home);
    for suffix in [".local/bin", ".bun/bin", ".volta/bin", ".local/share/mise/shims", ".asdf/shims", ".npm-global/bin", ".pyenv/shims", ".cargo/bin"]
    {
      dirs.push(home.join(suffix));
    }
    append_child_bins(home.join(".nvm/versions/node"), &mut dirs);
    append_child_bins(home.join(".pyenv/versions"), &mut dirs);
    // fnm stores the binary under <version>/installation/bin.
    if let Ok(entries) = fs::read_dir(home.join(".local/share/fnm/node-versions")) {
      for entry in entries.flatten() {
        dirs.push(entry.path().join("installation/bin"));
      }
    }
  }
  if let Some(path) = env::var_os("CODEX_INSTALL_DIR") {
    if Path::new(&path).is_absolute() {
      dirs.push(PathBuf::from(path));
    }
  }
  if let Some(path) = env::var_os("CARGO_HOME").filter(|path| Path::new(path).is_absolute()) {
    dirs.push(PathBuf::from(path).join("bin"));
  }
  for prefix in ["/opt/homebrew", "/usr/local"] {
    dirs.push(PathBuf::from(prefix).join("bin"));
    if let Ok(entries) = fs::read_dir(PathBuf::from(prefix).join("opt")) {
      let mut paths: Vec<_> = entries
        .flatten()
        .filter(|e| {
          let name = e.file_name();
          let name = name.to_string_lossy();
          name.starts_with("node@") || name.starts_with("python@") || name == "rust"
        })
        .map(|e| e.path().join("bin"))
        .collect();
      paths.sort();
      dirs.extend(
        paths
          .iter()
          .filter(|path| path.to_string_lossy().contains("/python@"))
          .filter_map(|path| path.parent().map(|parent| parent.join("libexec/bin"))),
      );
      dirs.extend(paths);
    }
  }
  append_child_bins(PathBuf::from("/Library/Frameworks/Python.framework/Versions"), &mut dirs);
  dirs.extend([PathBuf::from("/usr/bin"), PathBuf::from("/bin")]);
  dirs
}
