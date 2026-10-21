#!/bin/bash
set -euo pipefail

project_dir="$(cd "$(dirname "$0")/.." && pwd)"
cd "$project_dir"

for variable in APPLE_CERTIFICATE_BASE64 APPLE_CERTIFICATE_PASSWORD APPLE_ID APPLE_TEAM_ID APPLE_APP_SPECIFIC_PASSWORD; do
  test -n "${!variable:-}" || { echo "Missing release secret: $variable" >&2; exit 1; }
done

app_dir="$project_dir/dist/TendKit.app"
test -d "$app_dir" || { echo "Build TendKit.app before notarizing." >&2; exit 1; }
signing_dir="$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/tendkit-signing.XXXXXX")"
keychain_path="$signing_dir/signing.keychain-db"
original_keychains=()
while IFS= read -r line; do
  if [[ "$line" =~ ^[[:space:]]*\"(.*)\"[[:space:]]*$ ]]; then
    original_keychains+=("${BASH_REMATCH[1]}")
  fi
done < <(security list-keychains -d user)
cleanup() {
  security delete-keychain "$keychain_path" >/dev/null 2>&1 || true
  if [[ ${#original_keychains[@]} -gt 0 ]]; then
    security list-keychains -d user -s "${original_keychains[@]}" >/dev/null 2>&1 || true
  fi
  rm -rf "$signing_dir"
}
trap cleanup EXIT
trap 'echo "Signing or notarization failed at line $LINENO." >&2' ERR
umask 077

keychain_password="$(openssl rand -hex 32)"
printf '%s' "$APPLE_CERTIFICATE_BASE64" | base64 --decode > "$signing_dir/certificate.p12"
curl --fail --silent --show-error --location \
  https://www.apple.com/certificateauthority/DeveloperIDG2CA.cer \
  --output "$signing_dir/DeveloperIDG2CA.cer"
security create-keychain -p "$keychain_password" "$keychain_path"
security set-keychain-settings -lut 21600 "$keychain_path"
security unlock-keychain -p "$keychain_password" "$keychain_path"
security list-keychains -d user -s "$keychain_path" "${original_keychains[@]}"
security import "$signing_dir/DeveloperIDG2CA.cer" -k "$keychain_path"
security import "$signing_dir/certificate.p12" -k "$keychain_path" \
  -P "$APPLE_CERTIFICATE_PASSWORD" -T /usr/bin/codesign
security set-key-partition-list -S apple-tool:,apple: -k "$keychain_password" "$keychain_path" >/dev/null
identity="$(security find-identity -v -p codesigning "$keychain_path" | \
  awk -v team="$APPLE_TEAM_ID" 'index($0, "Developer ID Application:") && index($0, "(" team ")") { print $2 }')"
[[ "$identity" =~ ^[[:xdigit:]]{40}$ ]] || { echo "Expected one valid Developer ID Application identity for APPLE_TEAM_ID." >&2; exit 1; }

# Sign nested code first, then seal the app with hardened runtime and secure timestamps.
codesign --force --sign "$identity" --keychain "$keychain_path" --options runtime --timestamp \
  "$app_dir/Contents/Helpers/tendkit-engine"
codesign --force --sign "$identity" --keychain "$keychain_path" --options runtime --timestamp "$app_dir"
codesign --verify --deep --strict --verbose=2 "$app_dir"

ditto -c -k --sequesterRsrc --keepParent "$app_dir" "$signing_dir/notarization.zip"
xcrun notarytool submit "$signing_dir/notarization.zip" \
  --apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" --password "$APPLE_APP_SPECIFIC_PASSWORD" \
  --wait --timeout 20m --output-format json > "$signing_dir/notarization.json"
status="$(ruby -rjson -e 'puts JSON.parse(File.read(ARGV[0])).fetch("status")' "$signing_dir/notarization.json")"
if [[ "$status" != Accepted ]]; then
  echo "Apple notarization was not accepted (status: $status). No release will be published." >&2
  exit 1
fi
xcrun stapler staple "$app_dir"
xcrun stapler validate "$app_dir"
spctl --assess --type execute --verbose=2 "$app_dir"
echo "TendKit.app is signed, notarized, and verified for distribution."
