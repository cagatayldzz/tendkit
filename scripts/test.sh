#!/bin/bash
set -euo pipefail
project_dir="$(cd "$(dirname "$0")/.." && pwd)"
cd "$project_dir"

if [[ -x "$project_dir/.tools/cargo/bin/cargo" ]]; then
  export CARGO_HOME="$project_dir/.tools/cargo"
  export RUSTUP_HOME="$project_dir/.tools/rustup"
  export PATH="$CARGO_HOME/bin:$PATH"
fi
# Core integration tests drive the real engine binary.
cargo build --locked -p tendkit-engine
cargo test --locked --workspace
