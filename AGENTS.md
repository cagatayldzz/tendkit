# Project language

Use English for all project-owned identifiers, comments, UI text, logs,
diagnostics, test descriptions and fixtures, documentation, and build scripts.
Keep the application's development language set to English (`en`).
Do not add translations or other-language copies unless explicitly requested.
The conversation can follow the user's preferred language.

# Formatting

Use 2 spaces for indentation in all project-owned files, with LF line endings
and a final newline. Keep EditorConfig, VS Code, and `rustfmt.toml` aligned with
this rule. Run `cargo fmt --all` after Rust changes.

# Workspace layout

The Cargo workspace has `apps/*` (executables) and `crates/*` (libraries). The
dependency direction is enforced by `bun run check:layers`:

- `apps/tendkit` is the GPUI application; `apps/tendkit-engine` is the engine
  executable that the application bundles and starts as a child process.
- `crates/protocol` holds the wire models shared by both sides and depends on
  nothing else in the workspace.
- Engine layers: `process` → `detect` → `plan` → `execute` (plus `shellrc`).
  They never depend on the client, model, or UI crates.
- Application layers: `storage` and `client` → `model` → `ui`. They never depend
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

# Checks

Run `bun run check` before finishing a change (layers, script tests, formatting,
clippy, and Rust tests). Tests never install or modify real tools. Commit or push
only when the user asks.
