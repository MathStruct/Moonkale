---
title: "core — implementation notes"
tags: [crate-notes, milestone-1]
---
Implementation notes for `moonkale-core` (Milestone 1). Design: [[Graph-Native Model]]. Log: [[Milestone 1 - Implementation Log]].

## What is implemented
- `id.rs` — `SourceId(String)`, `NodeId(Uuid)`. `NodeId::derive(source, native_key)` is UUID v5 over `source ‖ 0x00 ‖ key` under a fixed namespace: deterministic, no lookup table, and the NUL separator prevents `("ab","c")` colliding with `("a","bc")` (tested).
- `graph/node.rs` — `Node { id, source, kind, label, native_key, content, version }`, `NodeKind` (open enum, `Custom(String)`), `ContentRef::{Text, Blob}`, `Version(u64)`. `language_hint()` is a temporary extension→language map until the index exists.
- `graph/edge.rs` — `Edge`, `EdgeKind`; only `Contains` is produced today.
- `source/query.rs` — `Query::{Node, Children}` → `QueryResult { nodes, edges }`.
- `source/transaction.rs` — `Transaction`/`Op::WriteText`/`TextPatch::Replace(Vec<Splice>)` with **char** offsets (tested on non-ASCII), `Applied` with per-op `OpResult`.
- `source/mod.rs` — the `Source` trait, async via `async-trait`.
- `error.rs` — `SourceError` (serializable so it crosses the server boundary intact).

## Critical decisions
- **No I/O, no runtime, no Dioxus.** Dependencies: `serde`, `uuid` (v5 only — no RNG, works on wasm with the `js` feature), `async-trait`, `thiserror`. This is what a WASM extension will see.
- **`?Send` on wasm32.** `#[cfg_attr(not(wasm32), async_trait)] #[cfg_attr(wasm32, async_trait(?Send))]` on the trait and on every impl. The `MaybeSendSync` supertrait does the same for the trait object. `async_trait` is re-exported so implementors don't take the dependency.
- **`Version` is opaque.** Consumers compare for equality only; sources choose the scheme.
- **Errors inside `Applied`, not `Err`.** A refused op is data; the transaction call itself only fails on transport/protocol errors.

## Still stubs
`graph/property.rs`, `graph/view.rs`, `command/`, `source/event.rs` — design comments only.

## Tests
`cargo test -p moonkale-core` — 7 tests (id determinism/scoping/separator/display, patch application, char offsets, range errors).
