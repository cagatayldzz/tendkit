#!/bin/bash
# Minimal assertions for the shell tests. Each test file sources this and the script under test.

failures=0

assert_eq() {
  local expected="$1" actual="$2" label="$3"
  if [[ "$expected" != "$actual" ]]; then
    echo "FAIL: $label" >&2
    echo "  expected: $expected" >&2
    echo "  actual:   $actual" >&2
    failures=$((failures + 1))
  fi
}

assert_contains() {
  local needle="$1" haystack="$2" label="$3"
  if [[ "$haystack" != *"$needle"* ]]; then
    echo "FAIL: $label (missing: $needle)" >&2
    failures=$((failures + 1))
  fi
}

# assert_fails <label> <command...>: the command must exit non-zero.
assert_fails() {
  local label="$1"
  shift
  if ("$@") >/dev/null 2>&1; then
    echo "FAIL: $label (expected a failure)" >&2
    failures=$((failures + 1))
  fi
}

finish() {
  [[ "$failures" -eq 0 ]]
}
