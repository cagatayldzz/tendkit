fn main() {
  println!("cargo:rerun-if-env-changed=TENDKIT_ENGINE_PATH");
  println!("cargo:rustc-check-cfg=cfg(embedded_engine)");
  if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") || std::env::var("PROFILE").as_deref() != Ok("release") {
    return;
  }
  let path = std::env::var_os("TENDKIT_ENGINE_PATH").expect("Windows release builds require TENDKIT_ENGINE_PATH; build tendkit-engine first");
  let path = std::fs::canonicalize(path).expect("Embedded Windows engine must exist");
  println!("cargo:rerun-if-changed={}", path.display());
  println!("cargo:rustc-env=TENDKIT_ENGINE_PATH={}", path.display());
  println!("cargo:rustc-cfg=embedded_engine");
}
