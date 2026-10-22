use super::*;
use gpui_kit_assets::IconName;

#[test]
fn doctor_icons_are_embedded_in_the_application() {
  for icon in [IconName::Stethoscope, IconName::CircleMinus, IconName::Copy, IconName::TriangleAlert] {
    let path = icon.path();
    let bytes = Assets.load(&path).unwrap().unwrap_or_else(|| panic!("Missing icon asset: {path}"));
    assert!(!bytes.is_empty(), "Empty icon asset: {path}");
  }
}
