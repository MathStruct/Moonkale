---
title: "state — implementation notes"
tags: [crate-notes, milestone-18]
---
Notes for `moonkale-state` (Milestone 18 phase 3). Design: [[Internal State]], [[ADR-0014 One store for internal state]].

The **interface** of the store for Moonkale's own state, with no engine (those are in `moonkale-state-stores`), so `moonkale-ext-api` can depend on it.

- `lib.rs` — `StateStore { get, scan(prefix), write(Batch) }`, `Batch`/`Op`, `Entries`, `Durability` (`Durable` = fsync per commit, `Relaxed` = survives a killed process), `StateError`.
- `key.rs` — `Key`: string and `u64` parts, encoded so byte order is part order (`0x00` escaped as `0x00 0xFF`, strings end in `0x00 0x01`, numbers big-endian); `Key::reader` reads parts back.
- `record.rs` — `Record { TABLE, VERSION, migrate }`, versioned JSON envelopes `{"v", "data"}` (older versions migrate, newer ones are refused), `Typed`, `encode`/`decode`/`put_in`.
- `tables.rs` — the tables and their key layouts (settings, layout, agent_sessions, events, snapshots, projects, index).
- `memory.rs` — `MemoryStore` (tests; the simplest form of the contract).
- `testing.rs` — the **conformance suite** (`conformance`, `persistence`) every backend runs.
- `copy.rs` — `copy(from, to, tables)`: moving a store to another engine.

In use since phase 5.2: the layout record (`ext-api::settings::LayoutRecord`) through `Persistence::state`.
