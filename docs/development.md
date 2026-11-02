# Development

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
