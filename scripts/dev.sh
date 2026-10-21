#!/bin/bash
set -euo pipefail

project_dir="$(cd "$(dirname "$0")/.." && pwd)"
bash "$project_dir/scripts/build-app.sh" debug
open -n "$project_dir/dist/TendKit.app"
