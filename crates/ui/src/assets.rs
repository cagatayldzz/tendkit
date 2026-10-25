use gpui_kit::{AssetSource, Result, SharedString};
use std::borrow::Cow;

// Lucide icons TendKit uses beyond GPUI Kit's default component set. Only
// these SVGs are embedded, not the whole catalog.
gpui_kit_assets::icon_assets!(
  AppIcons,
  [
    Boxes,
    Code,
    Asterisk,
    CircleArrowDown,
    Circle,
    GitBranch,
    Hexagon,
    HardDrive,
    House,
    Folder,
    File,
    Layers,
    Link,
    Package,
    ShieldCheck,
    SlidersHorizontal,
    Stethoscope,
    CircleMinus,
    Copy,
    TriangleAlert,
    Terminal,
    Wrench,
    X,
    Zap
  ]
);

pub struct Assets;

impl AssetSource for Assets {
  fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
    if let Some(bytes) = AppIcons.load(path)? {
      return Ok(Some(bytes));
    }
    gpui_kit_assets::Assets.load(path)
  }

  fn list(&self, path: &str) -> Result<Vec<SharedString>> {
    let mut paths = gpui_kit_assets::Assets.list(path)?;
    paths.extend(AppIcons.list(path)?);
    paths.sort();
    paths.dedup();
    Ok(paths)
  }
}

#[cfg(test)]
#[path = "../tests/assets.rs"]
mod tests;
