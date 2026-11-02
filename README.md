# TendKit

A native macOS developer environment manager written in Rust: a GPU-rendered
[GPUI](https://gpui.rs) interface and a separate installation engine.

TendKit detects Homebrew, Node.js, npm, Bun, Python, Rust, Codex, Claude Code, and OpenCommit,
then builds an install or update plan for the tools you select. Nothing changes
until you approve the plan; progress, logs, and before/after versions appear live
and the operation can be cancelled at any time.

## Requirements

- Apple Silicon, macOS 15+
- Command Line Tools (Xcode is not required)
- Stable Rust and Cargo
- Bun

## Development

```sh
bun run dev       # build and launch the app
bun run build     # local release build → dist/TendKit.app
bun run check     # layers, test layout, formatting, clippy, and tests
```

Windows and Linux builds are inspection previews; tool installation and
automatic updates are macOS-only.

## Documentation

- [docs/tools.md](docs/tools.md) — how tools are managed, language, self-update
- [docs/development.md](docs/development.md) — commands, workspace packages, engine queries
- [docs/releasing.md](docs/releasing.md) — CI, release workflow, secrets
- [docs/architecture.md](docs/architecture.md) — crate layers and data flow
- [docs/protocol.md](docs/protocol.md) — engine protocol
- [docs/platform-support.md](docs/platform-support.md) — platform support
- [AGENTS.md](AGENTS.md) — project conventions
