# Engine protocol v1

Transport: UTF-8 JSON Lines over stdin/stdout. One request per engine process.
Diagnostics go to stderr. Commands: `hello`, `scan`, `latest`, `plan`, `execute`.

Request:

```json
{"protocol_version":1,"request_id":"unique-id","command":"scan"}
```

Response order: `hello`, nine `tool_detected` events, `run_finished`.
Every event contains `protocol_version`, matching `request_id`, monotonically
increasing `sequence` starting at 1, `type`, and `data`.

```json
{"protocol_version":1,"request_id":"unique-id","sequence":2,"type":"tool_detected","data":{"id":"node","installations":[{"path":"/opt/homebrew/bin/node","version":"26.11.1","source":"Homebrew","error":null}],"manageable":true}}
```

`manageable` tells the client whether TendKit can install or update the tool
from its current state and source. The engine owns these rules; clients must not
re-derive them. npm is never manageable on its own because it ships with Node.

Tool IDs: `homebrew`, `node`, `npm`, `bun`, `python`, `rust`, `codex`, `claude`, `opencommit`.
Paths resolving to the same executable are deduplicated. Candidate order follows
the engine environment's PATH, then known installation locations. The first
candidate is shown in the main list; this is not a promise about an existing
interactive shell's active version. No shell startup files are sourced.

An empty installation array means no executable was found in supported paths.
A nonempty installation with null version and an error means a binary was found
but could not be verified. A successful scan does not imply latest versions.

The client rejects unknown protocol versions, mismatching request IDs, invalid
sequence numbers, duplicate tools, and incomplete scans. Restart the process for
a new request. `hello` is also supported as a handshake-only request.

## Latest versions

The read-only `latest` command accepts exactly one tool ID in `selected`.
It emits `hello`, optional `log` events, one `latest_version` event containing
`id` and `latest_version`, then `run_finished`. Metadata uses the existing
installation source and channel resolver. Homebrew and npm use their official
release metadata directly. Errors produce an `error` event; no installation
plan is created and no tools or shell files are changed.

## Plans and execution

```json
{"protocol_version":1,"request_id":"plan-id","command":"plan","selected":["node","bun"],"configure_shell":true}
```

Planning reads the local state and official version metadata. It emits `hello`,
optional `log` events, `plan_ready` with `data.plan`, then `run_finished`.
No tools or shell files are changed by planning.

Plan fields: `selected`, `configure_shell`, `steps`, optional Homebrew `package`,
`shell_files`, `shell_paths`. Step fields: `id`, `title`, `action`, `target`,
`recipe`, `before`, `dependencies`. Actions: `install`, `update`, `refresh`, `skip`.
Recipes are typed operations, never arbitrary shell text. Native targets are
specific versions; Homebrew package metadata is checked again before mutation.

Execute sends the entire unmodified plan in the initial request's `plan` field.
The engine acquires a per-user mutation lock and reconstructs the plan against
current installations. A changed environment produces an `error` before any
installation runs. The engine, rather than the client, builds each command.

Execution events: `hello`, `step_started`, `log`, `step_finished`, optional
`external_action_required`, nine final `tool_detected` events, `run_finished`.
Step results include `id`, `status`, optional `before`, `after`, and `message`.
Result statuses: `succeeded`, `skipped`, `failed`, `blocked`, `cancelled`.
Terminal run statuses: `succeeded`, `failed`, `cancelled`. Independent steps can
continue after a failure. Already completed mutations are not rolled back.

Keep stdin open throughout execution. The same request ID and protocol version
must be included in control messages:

```json
{"protocol_version":1,"request_id":"execution-id","command":"cancel"}
{"protocol_version":1,"request_id":"execution-id","command":"continue"}
```

`cancel` requests process-group termination and prevents later steps. Closing
stdin also requests cancellation. `continue` acknowledges completion of an
external Installer operation. `external_action_required` includes `id`, `path`
to the verified Homebrew package, and a user-facing `message`. The client
opens the package using macOS Installer and waits for the user's acknowledgement;
the engine then independently checks Homebrew. The package is retained if this
external operation is cancelled while Installer might still be reading it.

An `error` event terminates an operation without `run_finished`. Partial results
must not be presented as an overall success. UI log buffers and child-process
output capture are bounded.
