//! Version comparison for update detection, shared by tool and app updates.

/// Whether `latest` is a newer release than `installed`. Versions compare by
/// their numeric components; anything that does not parse is never "newer", so
/// an unusual version string cannot produce a false update prompt.
pub fn is_newer(latest: &str, installed: &str) -> bool {
  match (components(latest), components(installed)) {
    (Some(latest), Some(installed)) => {
      let length = latest.len().max(installed.len());
      let pad = |parts: &[u64]| -> Vec<u64> { (0..length).map(|i| parts.get(i).copied().unwrap_or(0)).collect() };
      pad(&latest) > pad(&installed)
    }
    _ => false,
  }
}

/// The dot-separated numeric components before any `-` or `+` suffix.
fn components(version: &str) -> Option<Vec<u64>> {
  let core = version.trim().trim_start_matches('v').split(['-', '+']).next()?;
  core.split('.').map(|part| part.parse().ok()).collect()
}

#[cfg(test)]
mod tests;
