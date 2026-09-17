---
title: "Host API Reference"
tags: [extensions, reference]
---
The `Host` handle is the only door. Every method is capability-checked (see `ext-api/src/capability.rs`) and representable across the WASM boundary.

## Graph (`permissions.sources`)
| method | permission | returns |
|---|---|---|
| `query(Query) -> GraphView` | read | paged; iterate with `.next_page()` |
| `fetch(NodeId) -> Content` | read | text / blob / rows |
| `apply(Transaction) -> Applied` | write | per-op results; `Unsupported` is not an error |
| `subscribe(Filter) -> Stream<SourceEvent>` | read | ids + versions only |
| `sources() -> Vec<SourceDescriptor>` | read | capabilities, schema graph |

## Commands
`register_command(desc, handler)` (rarely needed — prefer the manifest), `execute(CommandId, args) -> Value` (runs *any* command, subject to its `when`).

## UI
`show_panel(id)`, `open_editor(NodeId, preferred: Option<EditorId>)`, `set_status(text)`, `notify(level, text)`, `ask(Prompt) -> Answer` (confirm / input / pick), `set_context_key(key, value)`.

For wasm panels, `PanelOutput` is a `ui::Tree`:
`column, row, text, heading, button(label, Action), input(bind), list(items), table(rows), embed(view_id), spacer`. `Action::command(id, args)` or `Action::open(NodeId)`. Diffs are computed by the host.

## Storage
`kv_get(key) / kv_set(key, value)` — per-extension, per-workspace, persisted. Size-limited (soft 1 MB); larger data belongs in a source.

## LLM (`permissions.llm`)
`embed(texts) -> Vec<Vector>`, `complete(prompt, tools) -> Stream<Chunk>`; routed through the user's provider config and [[LLM and RAG]] policy. An extension cannot bypass policy by calling providers directly (network is allow-listed).

## Network (`permissions.network`)
`fetch(Request) -> Response`; URL must match the manifest allow-list.

## Process (`permissions.process`, desktop/server only)
`spawn(cmd, args, cwd) -> Child` with stdout/stderr streams; the binary must be on an allow-list in user settings.

## Errors
`ExtError::{Denied(Capability), NotFound, Unsupported, Invalid(msg), Io(msg), UnknownCommand}` — always recoverable; never panic across the boundary.
