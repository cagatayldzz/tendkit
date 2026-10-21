# Architecture

TendKit is a Cargo workspace of small crates with a one-way dependency
direction. `bun run check:layers` reads each package's
`[package.metadata.layers]` table and fails on any dependency that is not
listed, on missing rules for new crates, and on workspace cycles.

## Layers

```text
apps/tendkit ──► ui ──► model ──► client ──┐
        │          │       └────► storage   ├──► protocol
        └──────────┴────────────► storage   │
                                            │
apps/tendkit-engine ──► execute ──► plan ──► detect ──► process ──┘
                           └──► shellrc
```

| Crate | Owns | Depends on |
| --- | --- | --- |
| `protocol` | `InstallPlan`, `InstallStep`, `Recipe`, `Installation`, events, `EventDecoder`, `EngineRequest` | serde |
| `storage` | data directory, atomic writes, latest-version cache, window size, OpenCommit config, zsh instruction block | serde |
| `client` | `EngineConnection` (spawn, cancel, acknowledge, watchdog), `EventValidator` | protocol |
| `model` | `AppModel`, `ToolRow`, scan/plan/execute state | client, storage, protocol, gpui-kit |
| `ui` | `Workspace`, views, assets, `platform` helpers, actions | model, storage, protocol, gpui-kit |
| `process` | `Control` (cancel/continue), `command`, `curl`, `search_dirs`, `Sink` | protocol |
| `detect` | tool registry (`TOOLS`), detection, version parsing | process, protocol |
| `plan` | recipe selection, dependency order, release resolution, `manageable` | detect, process, protocol |
| `shellrc` | managed PATH block for `.zprofile`/`.zshrc` | none |
| `execute` | step execution, Homebrew package install, shell configuration, mutation lock | plan, detect, process, shellrc, protocol |

## Process model

The engine stays a separate process. The application starts it with one
JSON Lines request on stdin and reads events from stdout (see
[PROTOCOL.md](PROTOCOL.md)). Process isolation keeps cancellation, the SIGTERM
watchdog, and the per-user mutation lock independent of the interface, and
lets the engine be tested without the GUI.

An engine operation runs on a background thread. Events cross a channel to a
GPUI foreground task that updates the `AppModel` entity and calls
`cx.notify()`.

## Conventions

- English for every identifier, comment, string, and document.
- The wire models live in one place (`protocol`); the engine and the client
  never define their own copies.
- `unsafe_code` is denied workspace-wide; the libc call sites are listed in
  [AGENTS.md](../AGENTS.md).
