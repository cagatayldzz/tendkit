#!/bin/bash
# Usage:
#   scripts/release.sh            request a release: dispatches the Release workflow for the default branch
#   scripts/release.sh prepare    (CI) choose the next version from commits and stamp the build inputs
#   scripts/release.sh notes      (CI) write dist/RELEASE_NOTES.md from commit titles
#   scripts/release.sh package    (CI, any platform) package the build of this host as dist/TendKit-<tag>-<os>-<arch>.*
#   scripts/release.sh finalize   (CI) check the assets and write dist/SHA256SUMS.txt
#
# The version follows Conventional Commits: a breaking change is major, `feat` is minor, anything else is patch.
# The script can be sourced (the tests do) without running anything.

set -euo pipefail

usage="Usage: scripts/release.sh [prepare|notes|package|finalize]"
release_targets=("macos-arm64.zip" "macos-x64.zip" "windows-x64.zip" "linux-x64.tar.gz" "linux-x64.deb")
sections=("New Features" "Bug Fixes" "Documentation" "Chores")

fail() {
  echo "$1" >&2
  exit 1
}

# Pure helpers.

# Read full commit messages separated by NUL bytes on stdin and print major, minor or patch.
release_bump() {
  local message bump="patch"
  while IFS= read -r -d '' message; do
    if printf '%s\n' "$message" | grep -Eiq '^[a-z]+(\([^)]+\))?!:' || printf '%s\n' "$message" | grep -Eq '^BREAKING[ -]CHANGE:[[:space:]]'; then
      echo major
      return
    fi
    if printf '%s\n' "$message" | grep -Eiq '^feat(\([^)]+\))?:'; then
      bump="minor"
    fi
  done
  echo "$bump"
}

# next_version <current> <major|minor|patch>
next_version() {
  local current="$1" bump="$2" major minor patch
  [[ "$current" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] || fail "Invalid stable version: $current"
  IFS=. read -r major minor patch <<<"$current"
  case "$bump" in
    major) echo "$((major + 1)).0.0" ;;
    minor) echo "$major.$((minor + 1)).0" ;;
    patch) echo "$major.$minor.$((patch + 1))" ;;
    *) fail "Release bump must be patch, minor or major." ;;
  esac
}

# artifact_names <tag>: one asset name per line.
artifact_names() {
  local tag="${1:-}" suffix
  [[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "Invalid release tag."
  for suffix in "${release_targets[@]}"; do
    echo "TendKit-$tag-$suffix"
  done
}

# note_section <subject>: the release notes section a commit title belongs to.
note_section() {
  case "$1" in
    feat:* | feat\(*) echo "New Features" ;;
    fix:* | fix\(*) echo "Bug Fixes" ;;
    docs:* | docs\(*) echo "Documentation" ;;
    *) echo "Chores" ;;
  esac
}

# render_notes <repository> <tag> <previous-tag or empty>: commits as "<sha>TAB<subject>" lines on stdin.
render_notes() {
  local repository="$1" tag="$2" previous="$3" base commits section sha subject printed
  base="https://github.com/$repository"
  commits="$(cat)"
  [[ -n "$commits" ]] || fail "There are no changes to release."
  for section in "${sections[@]}"; do
    printed=""
    while IFS=$'\t' read -r sha subject; do
      [[ "$(note_section "$subject")" == "$section" ]] || continue
      if [[ -z "$printed" ]]; then
        printf '## %s\n\n' "$section"
        printed=1
      fi
      printf -- '- %s ([%s](%s/commit/%s))\n' "$subject" "${sha:0:7}" "$base" "$sha"
    done <<<"$commits"
    [[ -z "$printed" ]] || echo
  done
  printf '## Changelog\n\n'
  if [[ -n "$previous" ]]; then
    printf '[Full Changelog: %s...%s](%s/compare/%s...%s)\n\n' "$previous" "$tag" "$base" "$previous" "$tag"
  else
    printf '[Full Changelog](%s/commits/%s)\n\n' "$base" "$tag"
  fi
}

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d ' ' -f 1
  else
    shasum -a 256 "$1" | cut -d ' ' -f 1
  fi
}

# Replace the first `version = "…"` after a matching header line, or every package whose name matches, in place.
rewrite() {
  local file="$1" tmp
  shift
  tmp="$(mktemp)"
  awk "$@" "$file" >"$tmp"
  cat "$tmp" >"$file"
  rm -f "$tmp"
}

# stamp_version <root> <version> <build-number>: write the version into every build input below <root>.
stamp_version() {
  local root="$1" version="$2" build="$3"
  [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "Invalid version: $version"
  [[ "$build" =~ ^[0-9]+$ ]] || fail "Invalid build number: $build"

  grep -q '^\[workspace\.package\]' "$root/Cargo.toml" || fail "Could not stamp [workspace.package] in Cargo.toml."
  rewrite "$root/Cargo.toml" -v v="$version" '
    /^\[/ { inside = ($0 == "[workspace.package]") }
    inside && !done && /^version = / { print "version = \"" v "\""; done = 1; next }
    { print }'

  grep -Eq '^name = "tendkit(-[a-z0-9]+)*"$' "$root/Cargo.lock" || fail "Could not stamp the TendKit packages in Cargo.lock."
  rewrite "$root/Cargo.lock" -v v="$version" '
    /^name = "tendkit(-[a-z0-9]+)*"$/ { ours = 1; print; next }
    ours && /^version = / { print "version = \"" v "\""; ours = 0; next }
    { ours = 0; print }'

  grep -q '^  "version": ' "$root/package.json" || fail "Could not stamp package.json."
  sed -E -i.bak "s/^(  \"version\": \")[^\"]+/\1$version/" "$root/package.json"

  local plist="$root/apps/tendkit/macos/Info.plist"
  sed -E -i.bak \
    -e "s#(<key>CFBundleShortVersionString</key><string>)[^<]+#\1$version#" \
    -e "s#(<key>CFBundleVersion</key><string>)[^<]+#\1$build#" "$plist"
  rm -f "$root/package.json.bak" "$plist.bak"
}

# finalize_assets <directory> <tag>: check the asset set, then write SHA256SUMS.txt.
finalize_assets() {
  local directory="$1" tag="$2" names name expected found
  names="$(artifact_names "$tag")"
  found="$(cd "$directory" && ls -1 | grep -E '\.(zip|tar\.gz|deb)$' | sort || true)"
  expected="$(printf '%s\n' "$names" | sort)"
  [[ "$found" == "$expected" ]] || fail "Release assets do not match the required platform matrix."
  : >"$directory/SHA256SUMS.txt"
  while IFS= read -r name; do
    [[ -s "$directory/$name" ]] || fail "Empty release asset: $name"
    printf '%s  %s\n' "$(sha256_of "$directory/$name")" "$name" >>"$directory/SHA256SUMS.txt"
  done <<<"$names"
}

# Commands.

stable_tags() {
  git tag --list 'v*' --sort=-version:refname | grep -E '^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$' || true
}

require_ci() {
  [[ "${GITHUB_ACTIONS:-}" == true ]] || fail "This command runs only in GitHub Actions. Use scripts/release.sh without arguments."
}

request_release() {
  command -v gh >/dev/null 2>&1 || fail "Install GitHub CLI (brew install gh), then run gh auth login."
  [[ -z "$(git status --porcelain)" ]] || fail "Commit and push all changes before releasing."
  local repository branch sha remote_sha
  repository="$(gh repo view --json nameWithOwner --jq .nameWithOwner)"
  branch="$(gh repo view --json defaultBranchRef --jq .defaultBranchRef.name)"
  [[ "$(git branch --show-current)" == "$branch" ]] || fail "Release from the default branch: $branch."
  sha="$(git rev-parse HEAD)"
  remote_sha="$(gh api "repos/$repository/commits/$branch" --jq .sha)"
  [[ "$sha" == "$remote_sha" ]] || fail "Local HEAD must match the pushed default branch. Push or pull before releasing."
  gh workflow run release.yml --repo "$repository" --ref "$branch"
  echo "Release requested from $branch. The version increment is automatic."
  echo "Follow progress: https://github.com/$repository/actions/workflows/release.yml"
}

prepare() {
  require_ci
  local sha previous range current bump version tag count
  sha="$(git rev-parse HEAD)"
  [[ -z "${EXPECTED_SHA:-}" || "$EXPECTED_SHA" == "$sha" ]] || fail "The checkout does not match the workflow source commit. Run the release again."
  previous="$(stable_tags | head -n 1)"
  range="HEAD"
  if [[ -n "$previous" ]]; then
    git merge-base --is-ancestor "$previous" HEAD || fail "The previous release tag is not an ancestor of HEAD."
    range="$previous..HEAD"
  fi
  count="$(git rev-list --count "$range")"
  [[ "$count" -gt 0 ]] || fail "There are no changes since the previous release."
  current="${previous#v}"
  [[ -n "$current" ]] || current="$(sed -n 's/^  "version": "\(.*\)",\{0,1\}$/\1/p' package.json | head -n 1)"
  bump="$(git log --format=%B%x00 "$range" | release_bump)"
  version="$(next_version "$current" "$bump")"
  tag="v$version"
  mkdir -p dist
  printf 'PREVIOUS_TAG=%s\nTAG=%s\n' "$previous" "$tag" >dist/release-context.env
  # Stamp build inputs in the disposable CI checkout. Tags remain the version source of truth.
  stamp_version "$PWD" "$version" "${GITHUB_RUN_NUMBER:?GITHUB_RUN_NUMBER is required to prepare a release.}"
  : "${GITHUB_OUTPUT:?GITHUB_OUTPUT is required to prepare a release.}"
  printf 'version=%s\ntag=%s\nsha=%s\n' "$version" "$tag" "$sha" >>"$GITHUB_OUTPUT"
  echo "Preparing $tag ($bump) from $count commits."
}

notes() {
  require_ci
  [[ -f dist/release-context.env ]] || fail "Run prepare before notes."
  local PREVIOUS_TAG TAG range
  # shellcheck disable=SC1091
  source dist/release-context.env
  range="HEAD"
  [[ -z "$PREVIOUS_TAG" ]] || range="$PREVIOUS_TAG..HEAD"
  git log --reverse --format='%H%x09%s' "$range" | render_notes "${GITHUB_REPOSITORY:?GITHUB_REPOSITORY is required}" "$TAG" "$PREVIOUS_TAG" >dist/RELEASE_NOTES.md
}

# package_windows <source folder> <archive>: zip the portable application with the PowerShell packager, which checks the
# archive's entries. The paths go in as arguments (never through the environment) so spaces and quotes survive.
package_windows() {
  local source="${1:-}" archive="${2:-}" script="$PWD/scripts/package-windows.ps1" native_source native_archive
  [[ -n "$source" && -n "$archive" ]] || fail "Windows packaging requires source and archive paths."
  native_source="$source"
  native_archive="$archive"
  if command -v cygpath >/dev/null 2>&1; then
    native_source="$(cygpath -w "$source")"
    native_archive="$(cygpath -w "$archive")"
    script="$(cygpath -w "$script")"
  fi
  powershell.exe -NoProfile -NonInteractive -File "$script" -SourcePath "$native_source" -ArchivePath "$native_archive"
  [[ -s "$archive" ]] || fail "Windows packaging did not produce an archive."
}

# package_linux <tag> <base name>: a tar.gz of the binaries and a Debian package with the same files under /opt.
package_linux() {
  local tag="$1" base="$2" stage="$PWD/dist/TendKit" deb="$PWD/dist/deb" name
  mkdir -p "$stage"
  cp target/release/TendKit target/release/tendkit-engine "$stage/"
  cp docs/platform-support.md "$stage/README.md"
  tar -czf "dist/$base.tar.gz" -C dist TendKit
  mkdir -p "$deb/DEBIAN" "$deb/opt/tendkit" "$deb/usr/bin" "$deb/usr/share/applications"
  for name in TendKit tendkit-engine; do
    install -m 755 "$stage/$name" "$deb/opt/tendkit/$name"
  done
  printf '#!/bin/sh\nexec /opt/tendkit/TendKit "$@"\n' >"$deb/usr/bin/tendkit"
  chmod 755 "$deb/usr/bin/tendkit"
  printf '%s\n' '[Desktop Entry]' 'Type=Application' 'Name=TendKit' 'Comment=Developer environment and disk inspection' \
    'Exec=tendkit' 'Icon=utilities-terminal' 'Terminal=false' 'Categories=Development;Utility;' \
    >"$deb/usr/share/applications/com.cagatayldzz.tendkit.desktop"
  printf '%s\n' 'Package: tendkit' "Version: ${tag#v}" 'Architecture: amd64' 'Maintainer: TendKit' 'Section: devel' 'Priority: optional' \
    'Depends: libc6 (>= 2.39), libstdc++6, libasound2t64, libfontconfig1, libfreetype6, libxkbcommon0, libxkbcommon-x11-0, libxcb1, libxcb-render0, libxcb-shape0, libxcb-xfixes0, libx11-6, libxi6, libegl1, libvulkan1, libwayland-client0, curl' \
    'Description: Developer environment and disk inspection (Linux preview)' >"$deb/DEBIAN/control"
  dpkg-deb --build --root-owner-group "$deb" "dist/$base.deb"
}

# host_target: the release target of the machine this runs on, as "<os>-<arch>".
host_target() {
  local os arch
  case "$(uname -s)" in
    Darwin) os=macos ;;
    Linux) os=linux ;;
    MINGW* | MSYS* | CYGWIN*) os=windows ;;
    *) fail "Unsupported release host: $(uname -s)/$(uname -m)" ;;
  esac
  case "$(uname -m)" in
    arm64 | aarch64) arch=arm64 ;;
    x86_64 | AMD64 | amd64) arch=x64 ;;
    *) fail "Unsupported release host: $(uname -s)/$(uname -m)" ;;
  esac
  case "$os-$arch" in
    macos-arm64 | macos-x64 | windows-x64 | linux-x64) echo "$os-$arch" ;;
    *) fail "Unsupported release host: $(uname -s)/$(uname -m)" ;;
  esac
}

package() {
  local tag="${RELEASE_TAG:-}" target base
  artifact_names "$tag" >/dev/null
  target="$(host_target)"
  base="TendKit-$tag-$target"
  mkdir -p dist
  case "$target" in
    macos-*)
      [[ -x dist/TendKit.app/Contents/Helpers/tendkit-engine ]] || fail "Build and notarize the macOS application first."
      ditto -c -k --sequesterRsrc --keepParent dist/TendKit.app "dist/$base.zip"
      ;;
    windows-*)
      mkdir -p dist/TendKit
      cp target/release/TendKit.exe dist/TendKit/
      package_windows "$PWD/dist/TendKit" "$PWD/dist/$base.zip"
      ;;
    linux-*) package_linux "$tag" "$base" ;;
  esac
}

finalize() {
  finalize_assets dist "${RELEASE_TAG:-}"
}

main() {
  cd "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
  [[ $# -le 1 ]] || fail "$usage"
  case "${1:-}" in
    "") request_release ;;
    prepare) prepare ;;
    notes) notes ;;
    package) package ;;
    finalize) finalize ;;
    *) fail "$usage" ;;
  esac
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  main "$@"
fi
