# Architecture

TendKit is a Cargo workspace of small crates with a one-way dependency
direction. `bun run check:layers` reads each package's
`[package.metadata.layers]` table and fails on any dependency that is not
listed, on missing rules for new crates, and on workspace cycles.

## Layers

```text
apps/tendkit ──► ui ──► model ──► client ──┐
        │          │       ├────► storage   ├──► protocol
        │          │       ├────► updater ──► version
        └──────────┴───────┴────► storage   │
                                            │
apps/tendkit-engine ──► execute ──► plan ──► detect ──► process ──┘
                           └──► shellrc
```

| Crate | Owns | Depends on |
| --- | --- | --- |
| `protocol` | `InstallPlan`, `InstallStep`, `Recipe`, `Installation`, events, `EventDecoder`, `EngineRequest` | serde |
| `storage` | data directory, atomic writes, latest-version cache, window size, OpenCommit config, zsh instruction block | serde |
| `version` | numeric version comparison | none |
| `i18n` | `Language`, system language detection, `t`/`tf` over `locales/*.txt` | none |
| `updater` | release lookup, download, verification, bundle swap | version |
| `client` | `EngineConnection` (spawn, cancel, acknowledge, watchdog), `EventValidator` | protocol |
| `model` | `AppModel`, `ToolRow`, tool update status, scan/plan/execute state, app update state | client, storage, updater, version, protocol, gpui-kit |
| `ui` | `Workspace` (Sidebar, StatusBar, Settings, Tag, Alert, Notification from GPUI Kit), assets, `platform` helpers, actions | model, storage, updater, protocol, gpui-kit |
| `process` | `Control` (cancel/continue), `command`, `curl`, `search_dirs`, `Sink` | protocol |
| `detect` | tool registry (`TOOLS`), detection, version parsing | process, protocol |
| `plan` | recipe selection, dependency order, release resolution, `manageable` | detect, process, protocol |
| `shellrc` | managed PATH block for `.zprofile`/`.zshrc` | none |
| `execute` | step execution, Homebrew package install, shell configuration, mutation lock | plan, detect, process, shellrc, protocol |

## Process model

The engine stays a separate process. The application starts it with one
JSON Lines request on stdin and reads events from stdout (see
[protocol.md](protocol.md)). Process isolation keeps cancellation, the SIGTERM
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

## Installation trust and version checks

Native Bun and Codex installs download the exact version's macOS release asset
from the official GitHub repository. Execution validates the release tag, asset
URL, and published SHA-256 digest before extracting only the expected binary.
Claude uses the exact version's official manifest and verifies the native
installer binary before running `install <version>`. Missing checksums fail
closed. HTTPS publisher metadata is the trust root; these checks do not provide
an independent signature against a compromised publisher account or server.
Homebrew packages additionally require the trusted leaf installer certificate
`Patrick Linnane (927JGANW46)`. Certificate rotation requires a reviewed update.
Archive binaries are staged beside their destination and replaced atomically.

The execution step runner and artifact runner are injectable for tests. Fakes
exercise cancellation, command failure, dependency blocking, checksum rejection,
and successful verification without installing real tools.

Version discovery is separate from management permission. Externally managed
installations can show release information while mutation remains blocked.
Homebrew installations retain their formula or cask version source; other
installations use the tool's default release source (not a manager-specific
channel). The model limits version lookup engines to three concurrent requests
and deduplicates queued tool IDs. Failed or expired lookups are unavailable for
update decisions; the detail view labels any retained version as cached and
shows its last successful check time. Successful entries expire after six hours;
failed requests retain the existing ten-minute retry cooldown.

Implicit Homebrew dependencies use `brew update --auto-update` with a one-hour
freshness interval. Explicit Homebrew selections force `brew update`. Formula
queries and installs disable automatic updates, so a plan refreshes metadata
once rather than refreshing it again for each package.

## Test layout

Rust test implementations live in each package's `tests/` directory, grouped
by subject. Tests needing private access are registered as external unit modules
using `#[cfg(test)]` and `#[path]`. Such packages disable automatic integration
test discovery and declare any integration targets explicitly. Packages with only
integration tests can use Cargo's default discovery. Production APIs stay private.
`bun run check:test-layout` checks source placement, descriptive names, and test
registration for both `apps/` and `crates/`; it runs in the full check and CI.

Developer builds (`debug_assertions`) disable application self-updates through
`model::APP_UPDATES_ENABLED`: startup checks, download and restart actions,
settings update pages, status badges, and notifications share that policy.
Tool version checks and tool updates remain available in every build.

## TendKit Doctor

Doctor is a read-only view of local scan evidence, with pure diagnostic rules in
`model::doctor` and GPUI Kit cards in `ui::doctor`. It reuses the engine scan and
adds no application-to-engine crate dependencies. Each `tool_detected` event
includes optional `path_entries`: the engine's inherited PATH, before discovery
adds known manager directories. The client checks that the context stays the
same throughout a response. Older engines without this field are shown as
having unknown PATH context, rather than as failing PATH checks.

The report distinguishes competing command names on PATH from installations
found only in known directories, version differences, and failed version probes.
PATH precedence uses the original directory order and never skips a broken first
binary to label a later one active. Canonical symlink duplicates remain collapsed
by detection. Missing optional tools are not health failures. No synthetic health
score or remote latest-version comparison is used. A report becomes complete
only after the engine response finishes successfully; partial or failed scans
cannot present passing results.

Doctor's Run Checks action starts a local scan without release lookups. The view
shows installation sources, versions, paths, explanations, and static diagnostic
commands that the user may copy into a terminal. It never runs repair commands,
installs tools, edits shell files, or deletes binaries. The scope text explains
that Finder's inherited PATH may differ from a terminal; aliases, functions,
relative PATH directories, and project-specific manager overrides are not resolved.
