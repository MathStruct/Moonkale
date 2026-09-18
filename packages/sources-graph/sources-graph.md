---
title: "sources-graph — implementation notes"
tags: [crate-notes, milestone-3]
---
Notes for `moonkale-sources-graph` (Milestone 3). Design: [[Data Sources]], [[Database Backends]].

Only **LadybugDB** is implemented (feature `ladybug`, `lbug` 0.20 — the continuation of Kuzu: embedded, Cypher). TypeDB/Helix/Falkor remain stubs.

- `ladybug.rs` — `LadybugSource::open(path)` opens the database **read-only** (`SystemConfig::read_only`), id `ladybug:<abs path>`. Schema as a graph: `Database` root → node tables (`Table`) → properties (`Column`, "name: TYPE (pk)"); rel tables become `EdgeKind::Custom(<rel name>)` edges between their endpoint tables (`CALL show_connection`). `Children(root)` lists node *and* rel tables (rel ones labelled "(rel)"); `Children(table)` lists properties; `Children(vertex)` is one hop (`MATCH (a)-[r]-(b) WHERE id(a) = internal_id(t, o)`).
- `Query::Text { dialect: "cypher" }` runs any statement (the read-only open refuses writes), capped at 500 rows. The result is a `Table` of `core::Value`s **and** the `NODE`/`REL`/recursive-rel columns as `Vertex` nodes (`v:<table_id>:<offset>`, label `Label(first string property)`) and `Custom` edges, so the Graph panel can draw the answer.
- **Data graph:** `Query::All { kinds: Some([Vertex]) }` runs `MATCH (a)-[r]->(b) RETURN a, r, b LIMIT n` plus `MATCH (n) RETURN n LIMIT n` (isolated nodes) and merges them — what the Graph panel's *Data* mode draws. Vertex keys are `v:<Label>:<table_id>:<offset>` and labels are the first string property (the primary key, usually), so a person is drawn as "Alice" in the `Person` colour.
- `lbug` is a sync C++ binding; every call runs on `spawn_blocking` with a fresh `Connection` (connections are cheap; the `Database` is shared behind an `Arc`).
- `is_ladybug_path()` (`.lbug .kuzu .kz`; a directory for databases created before 0.11, a file since) compiles everywhere and is what the Explorer, desktop `open_database` and the server's `open_any` use.

Build: `lbug` downloads a prebuilt static `liblbug` (adds ~40 MB to the binary; ~15 s first build). If the download fails it compiles the C++ sources with cmake — not needed on this machine. `LBUG_SHARED` / `LBUG_LIBRARY_DIR` link a system install.

Sample database: `cargo run -p moonkale-sources-graph --features ladybug --example seed_people -- ~/moonkale-sample/people.lbug` (people, cities, projects; database files are tied to the storage version, so generate rather than download).

Tests: `cargo test -p moonkale-sources-graph --features ladybug` (temp database: schema graph, Cypher → table + nodes/edges, one-hop, wrong dialect refused).
