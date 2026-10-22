# Project language

Use English for all project-owned identifiers, comments, UI text, logs,
diagnostics, test descriptions and fixtures, documentation, and build scripts.
Keep the application's development language set to English (`en`).
The conversation can follow the user's preferred language.

Interface translations are the one exception, and they live only in
`crates/i18n/locales/<code>.txt` (`en` is the source of truth). Every user-visible
string goes through `tendkit_i18n::t`/`tf`; add the key to every locale file
(`cargo test -p tendkit-i18n` fails on a missing key or a changed placeholder).
Engine and updater diagnostics stay English.

# Formatting

Use 2 spaces for indentation in all project-owned files, with LF line endings
and a final newline. Keep EditorConfig, VS Code, and `rustfmt.toml` aligned with
this rule. Run `cargo fmt --all` after Rust changes.

In Rust files, leave one blank line after import groups and between functions,
methods, structs, enums, traits, and implementations, including test helpers.
Keep attributes and documentation comments attached to the item they describe.

In TOML files, leave one blank line before a standalone comment block that
follows key/value entries. Keep the comment attached to the setting it explains;
comments immediately after a table header do not need an extra blank line.

# Workspace layout

The Cargo workspace has `apps/*` (executables) and `crates/*` (libraries). The
dependency direction is enforced by `bun run check:layers`:

- `apps/tendkit` is the GPUI application; `apps/tendkit-engine` is the engine
  executable that the application bundles and starts as a child process.
- `crates/protocol` holds the wire models shared by both sides and depends on
  nothing else in the workspace.
- Engine layers: `process` → `detect` → `plan` → `execute` (plus `shellrc`).
  They never depend on the client, model, or UI crates.
- Application layers: `storage`, `client`, and `updater` (→ `version`) → `model` → `ui`; `i18n` has no dependencies and is used by `model`, `ui`, and the app. They never depend
  on the engine crates.
- Every package declares its allowed dependencies in
  `[package.metadata.layers]`. Define the layer decision for each new crate or
  dependency there; do not disable `check:layers`.
- Keep `unsafe` out of the workspace (`unsafe_code = "deny"`). The few libc calls
  (signals, process groups, `flock`, `localtime_r`) live in `process`, `client`,
  `execute`, and `ui`'s `platform` module, each with an explicit `allow`.

# Interface

Build the interface with GPUI Kit (`gpui-kit`) and its Lucide icons. Update
`docs/architecture.md` when the architecture changes.

# Test layout

Keep all Rust test implementations in the owning package's `tests/` directory
(`crates/<crate>/tests/` or `apps/<app>/tests/`). Use descriptive file names such
as `detection.rs`, `artifact.rs`, or `app_settings.rs`; do not use `tests.rs`,
place test files under `src/`, or write inline test modules in production files.
Leave one blank line after the import group and between top-level test items
(functions, structs, and implementations). Keep attributes such as `#[test]`
attached to the item immediately below them.

For tests that need private implementation details, register the external file
from its owning source module with `#[cfg(test)]` and
`#[path = "../tests/<subject>.rs"] mod tests;`. Set `autotests = false` in
`[package]` so Cargo does not also compile these files as integration tests.
If that package also has integration tests, register each with an explicit
`[[test]]` target. Packages containing only integration tests may keep Cargo's
automatic discovery. Never expose production internals just to move test files.

`bun run check:test-layout` enforces this layout and checks that test files are
registered. Keep it enabled in `bun run check` and CI when adding packages or tests.

# Checks

Run `bun run check` before finishing a change (layers, test layout, script tests, formatting,
clippy, and Rust tests). Tests never install or modify real tools. Commit or push
only when the user asks.
