#!/bin/bash
set -euo pipefail

project_dir="$(cd "$(dirname "$0")/.." && pwd)"
cd "$project_dir"

if [[ -x "$project_dir/.tools/cargo/bin/cargo" ]]; then
  export CARGO_HOME="$project_dir/.tools/cargo"
  export RUSTUP_HOME="$project_dir/.tools/rustup"
  export PATH="$CARGO_HOME/bin:$PATH"
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "Rust was not found. Install the Rust toolchain and try again." >&2
  exit 1
fi

configuration="${1:-debug}"
case "$configuration" in
  debug) profile_flags=() ;;
  release) profile_flags=(--release) ;;
  *) echo "Usage: scripts/build-app.sh [debug|release]" >&2; exit 1 ;;
esac

resources="$project_dir/apps/tendkit/macos"
# Link both binaries for the oldest supported macOS rather than the build machine.
export MACOSX_DEPLOYMENT_TARGET="$(/usr/libexec/PlistBuddy -c 'Print :LSMinimumSystemVersion' "$resources/Info.plist")"
cargo build --locked ${profile_flags[@]+"${profile_flags[@]}"} -p tendkit -p tendkit-engine

binary_dir="$project_dir/target/$configuration"
app_dir="$project_dir/dist/TendKit.app"
rm -rf "$app_dir"
mkdir -p "$app_dir/Contents/MacOS" "$app_dir/Contents/Helpers" "$app_dir/Contents/Resources"
cp "$binary_dir/TendKit" "$app_dir/Contents/MacOS/TendKit"
cp "$binary_dir/tendkit-engine" "$app_dir/Contents/Helpers/tendkit-engine"
cp "$resources/Info.plist" "$app_dir/Contents/Info.plist"
# Development builds carry the DEV corner badge (scripts/make-icns.sh).
icon="AppIcon.icns"
[[ "$configuration" == debug ]] && icon="AppIcon-dev.icns"
cp "$resources/$icon" "$app_dir/Contents/Resources/AppIcon.icns"

# Local development signature. Distribution requires Developer ID + notarization.
# codesign reports replacing the linker's ad-hoc signature; show output only on failure.
sign() {
  local output
  output="$(codesign --force --sign - "$1" 2>&1)" || { echo "$output" >&2; return 1; }
}
sign "$app_dir/Contents/Helpers/tendkit-engine"
sign "$app_dir"
echo "Ready: $app_dir"
