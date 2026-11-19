#!/bin/bash
# Version selection: bump detection and increments.
set -euo pipefail
cd "$(dirname "$0")/../.."
source scripts/tests/lib.sh
source scripts/release.sh

assert_eq patch "$(printf 'fix: a\0chore: b\0' | release_bump)" "fix and chore are patch"
assert_eq patch "$(printf 'update things\0' | release_bump)" "unconventional titles are patch"
assert_eq minor "$(printf 'fix: a\0feat(ui): b\0' | release_bump)" "feat is minor"
assert_eq major "$(printf 'feat: a\0fix!: b\0' | release_bump)" "bang is major"
assert_eq major "$(printf 'feat(git)!: a\0' | release_bump)" "scoped bang is major"
assert_eq major "$(printf 'fix: a\n\nBREAKING CHANGE: gone\0' | release_bump)" "footer is major"
assert_eq patch "$(printf 'fix: mentions feat: in body\0' | release_bump)" "feat inside a fix title is patch"

assert_eq 1.0.0 "$(next_version 0.9.4 major)" "major resets minor and patch"
assert_eq 0.2.0 "$(next_version 0.1.7 minor)" "minor resets patch"
assert_eq 0.1.8 "$(next_version 0.1.7 patch)" "patch increments"
assert_fails "rejects prerelease versions" next_version 1.0.0-beta patch
assert_fails "rejects leading zeros" next_version 01.0.0 patch
assert_fails "rejects unknown bumps" next_version 1.0.0 huge

finish
