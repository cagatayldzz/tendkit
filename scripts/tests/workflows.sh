#!/bin/bash
# The workflows keep the guarantees the release process relies on.
set -euo pipefail
cd "$(dirname "$0")/../.."
source scripts/tests/lib.sh

ci="$(cat .github/workflows/ci.yml)"
release="$(cat .github/workflows/release.yml)"
action="$(cat .github/actions/build-release/action.yml)"

assert_contains "bash scripts/tests/run.sh" "$ci" "CI runs the shell tests"
assert_contains "-D warnings" "$ci" "CI treats warnings as errors"
if grep -Eq 'runs-on: (macos|windows)' <<<"$ci"; then
  echo "FAIL: CI runs on Ubuntu only" >&2
  failures=$((failures + 1))
fi

assert_contains "workflow_dispatch:" "$release" "release is started by hand"
assert_contains "github.ref_name == github.event.repository.default_branch" "$release" "release runs from the default branch only"
assert_contains "cancel-in-progress: false" "$release" "a running release is never cancelled"
assert_contains "needs: validate" "$release" "preparation waits for validation"
assert_contains "needs: [macos_checks, macos_arm64, macos_intel, windows, linux]" "$release" "verification waits for the checks and every build"
assert_contains "needs: verify" "$release" "publish waits for verification"
assert_contains "bash scripts/release.sh notes" "$release" "release notes come from the release script"
assert_contains "bash scripts/release.sh finalize" "$release" "assets are finalized by the release script"
assert_contains "--draft" "$release" "the release is created as a draft first"
assert_contains "cargo test --locked -p tendkit -p tendkit-ui -p tendkit-model" "$release" "the macOS application is tested while the builds run"
if [[ "$release" == *"sha256sum --check"*"gh release create"* ]]; then :; else
  echo "FAIL: checksums are verified before publishing" >&2
  failures=$((failures + 1))
fi
for job in macos_arm64 macos_intel windows linux; do
  [[ "$release" == *"  $job:"* ]] || { echo "FAIL: the release builds $job" >&2; failures=$((failures + 1)); }
done

assert_contains "bash scripts/smoke-engine.sh" "$action" "every build smoke tests the engine"
assert_contains "bash scripts/notarize-app.sh" "$action" "the macOS build notarizes the app"
assert_contains "bash scripts/release.sh package" "$action" "the build packages with the release script"
notarize_line="$(grep -n 'notarize-app.sh' <<<"$action" | head -n 1 | cut -d: -f1)"
package_line="$(grep -n 'release.sh package' <<<"$action" | head -n 1 | cut -d: -f1)"
[[ "$notarize_line" -lt "$package_line" ]] || { echo "FAIL: the app is signed before it is packaged" >&2; failures=$((failures + 1)); }
[[ "$action" != *"secrets."* ]] || { echo "FAIL: the build action must not read secrets" >&2; failures=$((failures + 1)); }
assert_contains "if: inputs.os == 'macos'" "$action" "signing runs on macOS only"

for variable in APPLE_CERTIFICATE_BASE64 APPLE_CERTIFICATE_PASSWORD APPLE_ID APPLE_TEAM_ID APPLE_APP_SPECIFIC_PASSWORD; do
  assert_contains "secrets.$variable" "$release" "release reads $variable"
done

# Only shell and Rust (and PowerShell for the Windows archive): no other scripting runtimes in automation.
for file in .github/workflows/*.yml .github/actions/*/action.yml scripts/*.sh; do
  if grep -Eq '(^|[^a-z])(bun|npm|node|python3?|ruby)( |$)' "$file"; then
    echo "FAIL: $file uses a runtime other than shell and Rust" >&2
    failures=$((failures + 1))
  fi
done

finish
