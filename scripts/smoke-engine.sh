#!/bin/bash
# Run the built engine once with a catalog scan and check that its answer follows the protocol. SMOKE_PROFILE picks the
# Cargo profile folder (release by default).
set -euo pipefail
cd "$(dirname "$0")/.."

fail() {
  echo "$1" >&2
  exit 1
}

engine="target/${SMOKE_PROFILE:-release}/tendkit-engine"
case "$(uname -s)" in MINGW* | MSYS* | CYGWIN*) engine="$engine.exe" ;; esac
[[ -x "$engine" ]] || fail "Build the engine first: $engine was not found."

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
printf '%s\n' '{"protocol_version":1,"request_id":"release-smoke","command":"scan","selected":[],"configure_shell":false}' >"$work/request"

status=0
"$engine" <"$work/request" >"$work/output" 2>"$work/errors" &
pid=$!
(sleep 60 && kill "$pid" 2>/dev/null) &
watchdog=$!
wait "$pid" || status=$?
kill "$watchdog" 2>/dev/null || true
wait "$watchdog" 2>/dev/null || true
[[ "$status" -eq 0 ]] || fail "Native engine failed ($status): $(cat "$work/errors")"

lines="$(wc -l <"$work/output" | tr -d ' ')"
[[ "$lines" -gt 0 ]] || fail "Native engine returned no events."
[[ "$(grep -c '"request_id":"release-smoke"' "$work/output")" == "$lines" ]] || fail "Native engine answered a different request."
! grep -q '"type":"error"' "$work/output" || fail "Native engine reported an error."
grep -q '"type":"hello"' "$work/output" || fail "Native engine returned an incomplete response: no hello."
detected="$(grep '"type":"tool_detected"' "$work/output" | sed -n 's/^{"data":{"id":"\([^"]*\)".*/\1/p' | sort -u | wc -l | tr -d ' ')"
last="$(tail -n 1 "$work/output")"
[[ "$detected" == 9 && "$last" == *'"type":"run_finished"'* && "$last" == *'"status":"succeeded"'* ]] || fail "Native engine did not complete the catalog scan."
echo "Native engine protocol verified on $(uname -s)/$(uname -m)."
