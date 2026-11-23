#!/bin/bash
set -euo pipefail

project_dir="$(cd "$(dirname "$0")/.." && pwd)"
app_dir="$project_dir/dist/TendKit.app"
binary="$app_dir/Contents/MacOS/TendKit"
identifier="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$project_dir/apps/tendkit/macos/Info.plist")"

# `open` only brings an already running instance forward, so quit the running
# development app first: politely, then by force.
stop_running_app() {
  local attempt
  pgrep -f "$binary" >/dev/null 2>&1 || return 0
  osascript -e "tell application id \"$identifier\" to quit" >/dev/null 2>&1 || true
  for attempt in 1 2 3 4 5 6 7 8 9 10; do
    pgrep -f "$binary" >/dev/null 2>&1 || return 0
    sleep 0.3
  done
  pkill -f "$binary" >/dev/null 2>&1 || true
  sleep 0.5
}

bash "$project_dir/scripts/build-app.sh" debug
stop_running_app
open "$app_dir"
