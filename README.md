# TendKit

A native macOS developer environment manager written in Rust: a GPU-rendered
[GPUI](https://gpui.rs) interface and a separate installation engine.

TendKit detects Homebrew, Node.js, npm, Bun, Python, Rust, Codex, Claude Code, and OpenCommit,
then builds an install or update plan for the tools you select. Nothing changes
until you approve the plan; progress, logs, and before/after versions appear live
and the operation can be cancelled at any time.

## How it works

- **Homebrew** — downloads the official `.pkg`, verifies its SHA-256 checksum and
  signature, then opens the macOS Installer. TendKit never asks for a password.
- **Node/npm** — managed through Homebrew. An existing `node@MAJOR` formula is kept.
- **Python** — detected with `python3` and managed through Homebrew. Existing versioned Homebrew formulae are kept; installations from other sources stay with their current manager.
- **Rust** — detected with `rustc` and managed through Homebrew. Existing rustup and other externally managed installations are shown and left with their current manager.
- **Bun, Codex, Claude Code** — updated through their existing source (official
  installer or Homebrew), pinned to the approved version.
- **OpenCommit** — installed with Bun or an existing npm installation; its
  configuration is preserved.
- **External managers** (nvm, fnm, Volta, mise) — shown, but never migrated.
- **PATH** — optional. `.zprofile` and `.zshrc` are backed up to
  `~/Library/Application Support/TendKit/backups`, and only the TendKit block
  is edited.

Up-to-date tools are not reinstalled, and only one engine can run at a time.

## Requirements

- Apple Silicon, macOS 15+
- Command Line Tools (Xcode is not required)
- Stable Rust and Cargo
- Bun

## Development

```sh
bun run dev       # build and launch the app
bun run build     # local release build → dist/TendKit.app
bun run update    # update Cargo.lock for all workspace dependencies within manifest constraints
bun run check     # layers, test layout, formatting, clippy, and tests
bun run test      # script and Rust tests
bun run icons     # regenerate the app icons
```

The Cargo workspace separates executables (`apps/`) from libraries (`crates/`):

| Package | Role |
| --- | --- |
| `apps/tendkit` | GPUI application: window, menus, actions |
| `apps/tendkit-engine` | installation engine executable |
| `crates/protocol` | engine wire models and event decoding |
| `crates/client` | engine process connection and event validation |
| `crates/storage` | per-user files: caches, window size, app settings, OpenCommit config |
| `crates/updater` | TendKit self-update from GitHub releases |
| `crates/version` | version comparison |
| `crates/i18n` | interface languages (English, Türkçe) and system language detection |
| `crates/model` | application state (`AppModel`) |
| `crates/ui` | views built with [GPUI Kit](https://gpui-kit.com) |
| `crates/process` | supervised commands, downloads, search paths |
| `crates/detect` | tool registry and installation detection |
| `crates/plan` | planning and release resolution |
| `crates/shellrc` | managed PATH block for zsh files |
| `crates/execute` | plan execution |

`cargo run -p tendkit` runs the interface directly once the engine is built. A
Rust toolchain under `.tools` is picked up automatically if present.

Tests never install or modify real tools. To query the engine directly:

```sh
printf '%s\n' '{"protocol_version":1,"request_id":"manual","command":"scan"}' \
  | target/debug/tendkit-engine
```

## Language

On first launch TendKit uses the first language in your macOS language list that
it supports (currently English and Türkçe), and English when none is supported.
Change it in Settings → General → Interface language; "System default" follows
the computer again.

## Updating TendKit

Settings → Updates checks the latest GitHub release (on launch, unless turned
off, and on demand). Nothing is downloaded until you choose **Download & Install**.
The download must match the published `SHA256SUMS.txt`, carry a valid code
signature that macOS accepts (notarized), and be signed by the same developer
as the installed app. **Restart to Update** then swaps the app bundle and
reopens TendKit; a failed swap restores the previous version. The folder
containing TendKit.app must be writable.

## Releases

CI runs checks on every push to `main` and on pull requests. To publish a release,
run `bun run release` from a clean `main` (requires `gh auth login`), or start
**Actions → Release** manually.

The version bump is derived from commit messages since the last tag
(`feat:` → minor, `type!:` / `BREAKING CHANGE:` → major, otherwise patch).
Release notes are generated from commit titles with OpenAI. Native build jobs
produce macOS Apple Silicon and Intel ZIPs, a Windows x64 ZIP, and Linux x64
TAR/DEB packages. Both macOS apps are signed with Developer ID and notarized.
A combined `SHA256SUMS.txt` covers all five packages. Nothing is published unless
the checks and every native build and engine protocol smoke test pass.

Windows and Linux are inspection previews: tool installation and automatic app
updates remain macOS-only. Windows packages are unsigned. See
[platform support](docs/platform-support.md) for installation, runtime
requirements, and the exact support boundaries.

Required repository secrets:

| Secret | Purpose |
| --- | --- |
| `OPENAI_API_KEY` | Release notes |
| `APPLE_CERTIFICATE_BASE64` | Base64-encoded Developer ID Application `.p12` |
| `APPLE_CERTIFICATE_PASSWORD` | Password for the `.p12` |
| `APPLE_ID` | Apple Developer account email |
| `APPLE_TEAM_ID` | Team identifier |
| `APPLE_APP_SPECIFIC_PASSWORD` | App-specific password for notarization |

## Documentation

- [docs/architecture.md](docs/architecture.md) — crate layers and data flow
- [docs/protocol.md](docs/protocol.md) — engine protocol
- [AGENTS.md](AGENTS.md) — project conventions
