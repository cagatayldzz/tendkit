#!/bin/bash
# Run every shell test in scripts/tests (release_*.sh, notarize.sh, workflows.sh). Exit non-zero if any fails.
set -uo pipefail
cd "$(dirname "$0")/../.."

status=0
for test_file in scripts/tests/release_*.sh scripts/tests/notarize.sh scripts/tests/workflows.sh; do
  if bash "$test_file"; then
    echo "ok   $test_file"
  else
    echo "FAIL $test_file" >&2
    status=1
  fi
done
exit "$status"
