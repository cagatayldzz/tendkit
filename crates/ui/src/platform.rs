// Local time formatting uses localtime_r.
#![allow(unsafe_code)]

use std::{process::Command, sync::OnceLock, time::SystemTime};

pub fn channel() -> &'static str {
  if cfg!(debug_assertions) {
    "Developer"
  } else {
    "Stable"
  }
}

pub fn architecture() -> &'static str {
  match (std::env::consts::OS, std::env::consts::ARCH) {
    ("macos", "aarch64") => "Apple Silicon",
    ("macos", "x86_64") => "Intel",
    (_, arch) => arch,
  }
}

pub fn os_name() -> &'static str {
  match std::env::consts::OS {
    "macos" => "macOS",
    "windows" => "Windows",
    "linux" => "Linux",
    other => other,
  }
}

/// The operating system version, read once.
pub fn os_version() -> &'static str {
  static VERSION: OnceLock<String> = OnceLock::new();
  VERSION.get_or_init(|| {
    let output = if cfg!(target_os = "macos") {
      Command::new("/usr/bin/sw_vers").output().ok().map(|o| {
        let text = String::from_utf8_lossy(&o.stdout).into_owned();
        let field = |name: &str| text.lines().find_map(|l| l.strip_prefix(name)).map(str::trim).unwrap_or("").to_string();
        format!("Version {} (Build {})", field("ProductVersion:"), field("BuildVersion:"))
      })
    } else {
      Command::new("uname").arg("-r").output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    };
    output.filter(|v| !v.is_empty()).unwrap_or_else(|| "—".into())
  })
}

/// A local wall-clock time such as "14:05".
pub fn local_time(time: SystemTime) -> String {
  let seconds = time.duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
  #[cfg(unix)]
  unsafe {
    let raw = seconds as libc::time_t;
    let mut parts: libc::tm = std::mem::zeroed();
    if !libc::localtime_r(&raw, &mut parts).is_null() {
      return format!("{:02}:{:02}", parts.tm_hour, parts.tm_min);
    }
  }
  format!("{:02}:{:02} UTC", seconds / 3600 % 24, seconds / 60 % 60)
}

/// Groups thousands with commas, e.g. token limits as "32,000".
pub fn grouped(value: u32) -> String {
  let digits = value.to_string();
  let mut output = String::new();
  for (index, c) in digits.chars().enumerate() {
    if index > 0 && (digits.len() - index).is_multiple_of(3) {
      output.push(',');
    }
    output.push(c);
  }
  output
}

/// English language names for OpenCommit's language codes.
pub fn language_name(code: &str) -> String {
  let name = match code {
    "en" => "English",
    "cs" => "Czech",
    "de" => "German",
    "es_ES" => "Spanish (Spain)",
    "fr" => "French",
    "id_ID" => "Indonesian (Indonesia)",
    "it" => "Italian",
    "ja" => "Japanese",
    "ko" => "Korean",
    "nl" => "Dutch",
    "pl" => "Polish",
    "pt_br" => "Portuguese (Brazil)",
    "ru" => "Russian",
    "sv" => "Swedish",
    "th" => "Thai",
    "tr" => "Turkish",
    "vi_VN" => "Vietnamese (Vietnam)",
    "zh_CN" => "Chinese (China)",
    "zh_TW" => "Chinese (Taiwan)",
    other => other,
  };
  format!("{name} ({code})")
}

#[cfg(test)]
mod tests {
  #[test]
  fn groups_thousands() {
    assert_eq!(super::grouped(4_096), "4,096");
    assert_eq!(super::grouped(128_000), "128,000");
    assert_eq!(super::grouped(512), "512");
  }
}
