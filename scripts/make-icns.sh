#!/bin/bash
# Regenerates apps/tendkit/macos/AppIcon.icns and AppIcon-dev.icns from scripts/make-icon.swift.
set -euo pipefail

project_dir="$(cd "$(dirname "$0")/.." && pwd)"
resources="$project_dir/apps/tendkit/macos"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# render <name> <output.icns> [make-icon.swift flags...]
render() {
  local name="$1" output="$2"; shift 2
  swift "$project_dir/scripts/make-icon.swift" "$@" "$work/$name.png" >/dev/null
  local iconset="$work/$name.iconset"
  mkdir -p "$iconset"
  for size in 16 32 128 256 512; do
    sips -z "$size" "$size" "$work/$name.png" --out "$iconset/icon_${size}x${size}.png" >/dev/null
    sips -z "$((size * 2))" "$((size * 2))" "$work/$name.png" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
  done
  iconutil -c icns "$iconset" -o "$resources/$output"
}

mkdir -p "$resources"
render icon AppIcon.icns
render icon-dev AppIcon-dev.icns --dev
echo "Wrote $resources/AppIcon.icns and AppIcon-dev.icns"
