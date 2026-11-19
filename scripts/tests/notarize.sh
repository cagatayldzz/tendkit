#!/bin/bash
# Notarization gates distribution and removes the temporary signing keychain, with every macOS tool replaced by a recorder.
set -euo pipefail
cd "$(dirname "$0")/../.."
source scripts/tests/lib.sh

root="$(mktemp -d)"
trap 'rm -rf "$root"' EXIT
mkdir -p "$root/scripts" "$root/bin" "$root/dist/TendKit.app" "$root/temp"
cp scripts/notarize-app.sh "$root/scripts/"

cat >"$root/bin/mock" <<'MOCK'
#!/bin/bash
set -eu
command_name="${0##*/}"
printf '%s\n' "$command_name $*" >>"$COMMAND_LOG"
case "$command_name" in
  security)
    if [[ "$1" == list-keychains ]]; then
      if [[ "$#" == 3 ]]; then
        echo '    "/tmp/Original Keychains/login.keychain-db"'
      elif [[ "$5" == */signing.keychain-db ]]; then
        printf '%s' "$5" >"$COMMAND_LOG.active-keychain"
      fi
    fi
    if [[ "$1" == find-identity && "${INVALID_IDENTITY:-}" != true ]]; then
      echo '1) AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA "Developer ID Application: Test (TESTTEAM)"'
    fi
    ;;
  codesign)
    if [[ "$1" != --verify && ! -f "$COMMAND_LOG.active-keychain" ]]; then
      echo 'error: The specified item could not be found in the keychain.' >&2
      exit 1
    fi
    ;;
  xcrun)
    if [[ "$1 $2" == 'notarytool submit' ]]; then
      printf '{"status":"%s"}\n' "$NOTARY_STATUS"
    fi
    ;;
  spctl) exit "${ASSESSMENT_EXIT:-0}" ;;
  # plutil only exists on macOS: read the status out of the JSON the notarytool mock wrote.
  plutil) sed -n 's/.*"status":"\([^"]*\)".*/\1/p' "${@: -1}" ;;
esac
MOCK
for tool in security curl codesign ditto xcrun spctl plutil; do
  cp "$root/bin/mock" "$root/bin/$tool"
  chmod +x "$root/bin/$tool"
done
rm "$root/bin/mock"

log="$root/commands.log"
exit_code=0
commands=""

# run_notarize [NAME=value ...]: run the script and set exit_code and commands.
run_notarize() {
  : >"$log"
  rm -f "$log.active-keychain"
  exit_code=0
  (
    cd "$root"
    env PATH="$root/bin:$PATH" RUNNER_TEMP="$root/temp" COMMAND_LOG="$log" \
      APPLE_CERTIFICATE_BASE64=dGVzdA== APPLE_CERTIFICATE_PASSWORD=fixture-password APPLE_ID=test@example.com \
      APPLE_TEAM_ID=TESTTEAM APPLE_APP_SPECIFIC_PASSWORD=fixture-app-password NOTARY_STATUS=Accepted "$@" \
      bash scripts/notarize-app.sh
  ) >/dev/null 2>&1 || exit_code=$?
  commands="$(cat "$log")"
}

line_of() {
  grep -n -- "$1" <<<"$commands" | head -n 1 | cut -d: -f1
}

run_notarize
assert_eq 0 "$exit_code" "an accepted submission succeeds"
assert_contains "--options runtime --timestamp" "$commands" "the hardened runtime and a timestamp are used"
[[ "$(line_of 'Contents/Helpers/tendkit-engine')" -lt "$(line_of 'codesign --verify')" ]] || { echo "FAIL: the engine is signed before the app is verified" >&2; failures=$((failures + 1)); }
[[ "$(line_of 'notarytool submit')" -lt "$(line_of 'stapler staple')" ]] || { echo "FAIL: stapling follows notarization" >&2; failures=$((failures + 1)); }
assert_contains "stapler validate" "$commands" "the ticket is validated"
assert_contains "spctl --assess" "$commands" "Gatekeeper assesses the app"
assert_contains "security delete-keychain" "$commands" "the signing keychain is removed"
assert_contains "security list-keychains -d user -s /tmp/Original Keychains/login.keychain-db" "$commands" "the keychain list is restored"

run_notarize NOTARY_STATUS=Invalid
[[ "$exit_code" -ne 0 ]] || { echo "FAIL: a rejected submission fails" >&2; failures=$((failures + 1)); }
[[ "$commands" != *"stapler staple"* ]] || { echo "FAIL: a rejected app is never stapled" >&2; failures=$((failures + 1)); }
assert_contains "security delete-keychain" "$commands" "the keychain is removed after a rejection"

run_notarize ASSESSMENT_EXIT=1
[[ "$exit_code" -ne 0 ]] || { echo "FAIL: a failed Gatekeeper assessment fails" >&2; failures=$((failures + 1)); }

run_notarize INVALID_IDENTITY=true
[[ "$exit_code" -ne 0 ]] || { echo "FAIL: a missing identity fails" >&2; failures=$((failures + 1)); }
[[ "$commands" != *"notarytool submit"* ]] || { echo "FAIL: nothing is submitted without an identity" >&2; failures=$((failures + 1)); }

run_notarize APPLE_APP_SPECIFIC_PASSWORD=
[[ "$exit_code" -ne 0 ]] || { echo "FAIL: a missing secret fails" >&2; failures=$((failures + 1)); }
assert_eq "" "$commands" "no tool runs with a missing secret"

finish
