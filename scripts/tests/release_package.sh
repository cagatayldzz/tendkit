#!/bin/bash
# Version stamping, asset verification and the Windows packager call, on a temporary copy of the real build inputs.
set -euo pipefail
cd "$(dirname "$0")/../.."
source scripts/tests/lib.sh
source scripts/release.sh

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

mkdir -p "$work/apps/tendkit/macos"
cp Cargo.toml Cargo.lock package.json "$work/"
cp apps/tendkit/macos/Info.plist "$work/apps/tendkit/macos/"
stamp_version "$work" 7.8.9 42

assert_contains 'version = "7.8.9"' "$(awk '/^\[workspace\.package\]/{f=1} f' "$work/Cargo.toml" | head -n 3)" "workspace version"
assert_contains '"version": "7.8.9"' "$(cat "$work/package.json")" "package.json version"
assert_contains '<key>CFBundleShortVersionString</key><string>7.8.9</string>' "$(cat "$work/apps/tendkit/macos/Info.plist")" "plist version"
assert_contains '<key>CFBundleVersion</key><string>42</string>' "$(cat "$work/apps/tendkit/macos/Info.plist")" "plist build number"
names="$(grep -c '^name = "tendkit' "$work/Cargo.lock")"
stamped="$(grep -A1 '^name = "tendkit' "$work/Cargo.lock" | grep -c '^version = "7.8.9"')"
assert_eq "$names" "$stamped" "every TendKit package in the lockfile is restamped"
assert_eq "0" "$(diff Cargo.lock "$work/Cargo.lock" | grep '^[<>]' | grep -vc '^[<>] version = ' || true)" "only version lines changed in the lockfile"

assert_fails "rejects a bad version" stamp_version "$work" 1.0 1
assert_fails "rejects a bad build number" stamp_version "$work" 1.0.0 x

expected_names="TendKit-v1.2.3-macos-arm64.zip
TendKit-v1.2.3-macos-x64.zip
TendKit-v1.2.3-windows-x64.zip
TendKit-v1.2.3-linux-x64.tar.gz
TendKit-v1.2.3-linux-x64.deb"
assert_eq "$expected_names" "$(artifact_names v1.2.3)" "asset names"
assert_fails "rejects tags without v" artifact_names 1.2.3
assert_fails "rejects an empty tag" artifact_names ""
assert_fails "rejects a tag that escapes the folder" artifact_names "v1.2.3/../../file"

dist="$work/dist"
mkdir "$dist"
while IFS= read -r name; do
  [[ "$name" == *macos-arm64.zip ]] || echo payload >"$dist/$name"
done <<<"$expected_names"
assert_fails "a missing asset" finalize_assets "$dist" v1.2.3
: >"$dist/TendKit-v1.2.3-macos-arm64.zip"
assert_fails "an empty asset" finalize_assets "$dist" v1.2.3
echo payload >"$dist/TendKit-v1.2.3-macos-arm64.zip"
finalize_assets "$dist" v1.2.3
assert_eq "5" "$(wc -l <"$dist/SHA256SUMS.txt" | tr -d ' ')" "one checksum per package"
assert_eq "5" "$(grep -Ec '^[0-9a-f]{64}  TendKit-' "$dist/SHA256SUMS.txt")" "checksum lines name the packages"
echo extra >"$dist/unexpected.zip"
assert_fails "unexpected assets" finalize_assets "$dist" v1.2.3

# The Windows packager gets whole paths as arguments, even with spaces and apostrophes, and must leave an archive behind.
mkdir -p "$work/bin"
cat >"$work/bin/powershell.exe" <<'MOCK'
#!/bin/bash
printf '%s\n' "$@" >"$ARGS_LOG"
while [[ $# -gt 0 ]]; do
  [[ "$1" == -ArchivePath && -z "${SKIP_ARCHIVE:-}" ]] && printf 'archive fixture' >"$2"
  shift
done
MOCK
chmod +x "$work/bin/powershell.exe"
export ARGS_LOG="$work/args.log"
source_dir="$work/tend kit's source"
archive="$work/TendKit release.zip"
mkdir -p "$source_dir"
PATH="$work/bin:$PATH" package_windows "$source_dir" "$archive"
args="$(cat "$ARGS_LOG")"
assert_contains "-File" "$args" "the packager runs from a file"
assert_contains "$source_dir" "$args" "the source path is one argument"
assert_contains "$archive" "$args" "the archive path is one argument"
[[ "$args" != *"-Command"* ]] || { echo "FAIL: no -Command" >&2; failures=$((failures + 1)); }
assert_fails "no archive is an error" env SKIP_ARCHIVE=1 PATH="$work/bin:$PATH" bash -c 'source scripts/release.sh; package_windows "$1" "$2"' _ "$source_dir" "$work/missing.zip"
assert_fails "empty paths are an error" package_windows "" ""

finish
