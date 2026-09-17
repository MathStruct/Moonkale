//! `Host` — the extension's window onto the application.
//!
//! Every method here is (a) async-capable, (b) checked against the manifest's
//! permissions, (c) representable over the WASM boundary (plain data in, plain
//! data out). Roughly grouped:
//!
//! - **graph**: `query(Query)`, `fetch(NodeId)`, `apply(Transaction)`,
//!   `subscribe(filter)` — the same `core::source` surface, multiplexed over
//!   all open sources and permission-filtered.
//! - **commands**: `register_command`, `execute(CommandId, args)`.
//! - **ui**: `show_panel`, `set_status`, `notify`, `ask(Prompt)`,
//!   `open_editor(NodeId, preferred_editor)`.
//! - **context**: `set_context_key`, for `when` clauses.
//! - **storage**: `kv_get/kv_set` scoped to the extension (settings, caches).
//! - **llm** (permissioned): `embed(text)`, `complete(prompt, tools)` — routed
//!   through `moonkale-llm` so the user's provider config applies.
//! - **net** (permissioned): `fetch(url)` restricted to manifest allow-list.
//! - **process** (desktop only, permissioned): `spawn(cmd)`.
//!
//! For wasm extensions that contribute panels, the UI is described with a
//! small retained-mode tree (`ui::Tree`: rows, columns, text, buttons, lists,
//! tables, inputs, and an `Embed(view_id)` escape hatch for host-rendered
//! editors) and updated with diffs. This is *less* expressive than raw Dioxus
//! on purpose: it is what keeps wasm extensions portable across web/desktop/
//! mobile and sandboxed.
