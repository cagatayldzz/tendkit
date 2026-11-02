# Tools, language, and updates

## How tools are managed

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
