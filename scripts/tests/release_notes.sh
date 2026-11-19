#!/bin/bash
# Release notes: sections, links and the changelog footer.
set -euo pipefail
cd "$(dirname "$0")/../.."
source scripts/tests/lib.sh
source scripts/release.sh

assert_eq "New Features" "$(note_section 'feat(ui): tabs')" "feat section"
assert_eq "Bug Fixes" "$(note_section 'fix: crash')" "fix section"
assert_eq "Documentation" "$(note_section 'docs: readme')" "docs section"
assert_eq "Chores" "$(note_section 'refactor: split')" "other section"

sha_a="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
sha_b="bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
notes="$(printf '%s\tfeat: add tabs\n%s\tchore: tidy\n' "$sha_a" "$sha_b" | render_notes owner/tendkit v0.2.0 v0.1.0)"

assert_contains "## New Features" "$notes" "feature heading"
assert_contains "- feat: add tabs ([aaaaaaa](https://github.com/owner/tendkit/commit/$sha_a))" "$notes" "commit link"
assert_contains "## Chores" "$notes" "chores heading"
assert_contains "[Full Changelog: v0.1.0...v0.2.0](https://github.com/owner/tendkit/compare/v0.1.0...v0.2.0)" "$notes" "compare link"
[[ "$notes" != *"## Bug Fixes"* ]] || { echo "FAIL: empty sections are omitted" >&2; failures=$((failures + 1)); }

first="$(printf '%s\tfeat: first\n' "$sha_a" | render_notes owner/tendkit v0.1.0 '')"
assert_contains "[Full Changelog](https://github.com/owner/tendkit/commits/v0.1.0)" "$first" "first release changelog"
assert_fails "no commits means no release" bash -c 'source scripts/release.sh; printf "" | render_notes owner/tendkit v0.1.0 ""'

finish
