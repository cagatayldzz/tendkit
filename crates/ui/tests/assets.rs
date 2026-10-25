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

#[test]
fn disk_icon_is_embedded_in_the_application() {
  for icon in [IconName::HardDrive, IconName::House, IconName::Folder, IconName::File, IconName::Terminal] {
    assert!(Assets.load(&icon.path()).unwrap().is_some());
  }
}

#[test]
fn every_catalog_tool_icon_is_available() {
  for id in tendkit_protocol::TOOL_IDS {
    let path = crate::components::tool_icon_name(id).path();
    assert!(Assets.load(&path).unwrap().is_some(), "Missing tool icon: {id}");
  }
}
