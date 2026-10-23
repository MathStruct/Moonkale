# Sophia source (WP-68)

`moonkale-sources-sophia` implements `Source` over a Sophia code store, as a thin translation of `sophia-interface`'s types (Sophia SPEC-004, in [MathStruct/Sophia](https://github.com/MathStruct/Sophia) `vault/Specifications/`). Descriptor: `truth: Store`, family `Graph`, read, write and watch (polling), text dialects `sql` and `query` (`extra_text_query`), Sophia's own language. Enable the default-off `sophia` feature in `desktop`, `web` (client and server builds) or `moonkale-distribution`; the server has its own matching feature. This is a source driver, not an extension manifest.

## Dependency

`sophia-interface` and `sophia-store` come from git at one locked revision, which Sophia announces when it is dependable (SPEC-004 §3a); `deny.toml` allows that repository. Only the SQLite backend is built: it is the one with `sql`, and Sophia's redb backend would move Moonkale's own redb version. Bump the revision in `Cargo.toml` together with the tests below. Currently 4c2aaab3 (2026-10-21).

## Opening

The opener claims files named `*.sophia` (a Sophia SQLite store). Make one with Sophia's CLI: `cargo run -p sophia-cli -- --store work.sophia sample` builds SPEC-004 §9's sample. `SophiaSource::sample()` builds the same sample in memory, for tests. A missing file is `NotFound`; the source never creates a store.

The reader id is given once at open (SPEC-004 §4): `SophiaSource::open_as(path, reader)`. The opener uses `moonkale` for everyone until Moonkale has user ids. The rendering profile is stored for that reader in the Sophia database, never in Moonkale's settings.

## Mapping

- **Ids:** `Key::Content(d)` → `NodeId::from_content(d)`, `Key::Derived(s)` → `NodeId::derive(source, s)`. `native_key` is the derived key as is (`ns:a.b`, `name:a.b.f`, `settings`) or `#<hex>` for content.
- **Ids back to keys:** Moonkale hands back `NodeId`s, which can't be inverted, so the source remembers the key of every node and edge end it has answered with (cleared at a million). An id this process never answered with is `NotFound`; browse to it first. A restored tab from an earlier run therefore needs its node re-found by path.
- **Kinds:** `Directory` → `NodeKind::Directory`; the rest (`Name`, `Definition`, `sophia:Settings`, `sophia:Lineage`) are `NodeKind::Custom`. `Contains`, `References` and `Calls` map to the built-in edge kinds, everything else (`Names`, `MethodOf`, `Tests`, `Annotates`, `sophia:*`) to `EdgeKind::Custom`. Properties pass through.
- **Text:** names, definitions and the settings node carry `ContentRef::Text`, so the code editor opens them and the graph panel's store-text path applies. `fetch_text` of a name prints its whole recursive group in the reader's profile; a definition prints read-only (version 0).

## Edits

**Versions are edit tokens** (from revision 404b332b): the `Version` of a name's text is an opaque hash over every name that text covers (the name and all names of its recursive group, with their seqs and targets). Tokens aren't ordered; equal means unchanged. So saving `even`, whose text also holds `odd`, conflicts if `odd` changed meanwhile, instead of reverting it. A profile switch alone keeps the token. `WriteText` checks the token, applies the patch to that text and hands the whole text to Sophia, which parses it, stores the new definition and moves the name. The name's id doesn't change. Parse errors come back as one `SourceError::Invalid` with a `line:col: message` per diagnostic, and the name is unchanged. A stale version is `Conflict`; writing a definition rather than a name is `Unsupported`. **Aliases:** a definition opens read-only; it is edited through the name it was reached by (SPEC-004 §3). The source never picks a name for a bare definition, because with aliases in several namespaces that could move the wrong one. `CreateText`, `CreateDir`, `Rename` (Moonkale's `/` paths) and `Delete` map one to one. Sophia stores and reprints in the reader's profile, so the stored text can differ from what was typed (indentation, keywords). After a save to any `truth: Store` source the workspace fetches the text back and, when it differs, makes it the document's saved text, and its buffer unless it was edited meanwhile (`ext-api` `Workspace::save`, test `store_save.rs`). Without that, the next save's patch was against text the store no longer had (found in the desktop check, 2026-10-20). An edit is several store transactions on Sophia's side (Sophia SP-008): a failure half-way leaves a consistent, partial state.

## Change feed

`changes_since` polls Sophia's log every 500 ms for up to 25 s. The first call only returns the position. `paths` are the changed native keys, which the workspace resolves through `Query::Text { dialect: "path" }`; a gone name falls back to `derive(source, path)`, which is its old id. A rendering-profile change (or a log batch that didn't fit) is `reset`: everything is re-read, since every definition's text may have changed. Open names without unsaved edits are reprinted then too; the version stays, so the workspace compares the text.

## Queries

`Query::Text { dialect: "sql" }` runs on Sophia's documented views through its read-only guard (a read-only connection, an authorizer, 10 000 rows, a step budget) and answers a table plus the definitions its id columns name. Writes are refused by Sophia. `classify` keeps Moonkale's shared rules, so a write statement still counts as a write for the agent's policy.

`Query::Text { dialect: "query" }` (from revision 202e137a) runs Sophia's own query language (Sophia SPEC-007 §8) on any backend: a table, with ids as hex text, plus the definitions its id cells name. It is read-only by construction, so `classify("query")` is `Read`. A query with parameters can't run as text. The descriptor names both, so the table panel shows a button for each and the agent's `workspace.list_sources` prints `sql,query`. Other dialects are `Unsupported`.

**Views as tables** (2026-10-21): the root has a `views` folder (`moonkale:views`) with one `Table` node per Sophia SQL view (`sophia_store::query::VIEWS`, native key `table:<view>`), so the table panel opens them as it does a database's tables (`SELECT * FROM "definitions" LIMIT 1000`). They are the adapter's, not Sophia's: no text, read-only, not in the change feed or `Query::All`.

## Tests

`tests/acceptance.rs` runs SPEC-004 §9 through the `Source` trait only: the sample browses from the root; an edit moves the name to new content, and a parse error and a stale version leave it; a rename arrives through the change feed and resolves like the workspace resolves it; a profile switch reprints with no id change and resets; two clients on one file in two profiles see one id in two spellings, and a stale save through a recursive group conflicts instead of overwriting the other client's edit (Sophia's review demonstration 2); `sql` returns a table and nodes and refuses a write; `query` answers Sophia's transitive-users example and refuses a parse error; the views folder lists the nine views as tables, both default queries answer, and the views refuse edits; text creates names and directories; a store file opens. `moonkale-editor-graph`'s `sophia_names_and_definitions_reach_renderer` checks the graph panel's payload.

## Timing the edit loop

`examples/edit_loop.rs` is the loop Sophia's B-15 times from outside, through the `Source` trait only: resolve a name by path, fetch its text, save an edit with `apply`, read the name's new definition (`Names` edge) and the definitions still referencing the old one (`References` into it). `cargo run --release -p moonkale-sources-sophia --example edit_loop -- [--store <file.sophia> --name <a.b.f> --text <file>] [--runs <n>]`. It alternates the original and the edited text, prints µs per step for each run and the medians. On the in-memory sample (`sample.add`, 200 runs, 2026-10-20): fetch 55 µs, apply 188 µs, new id 18 µs, dependants 18 µs. It leaves out the workspace's fetch-back after a save and the UI.

**B-15 results** (Sophia's measurement, Moonkale 26752448, Sophia 4350543c, its vault `Plans/Benchmark Results.md`): the whole loop's p99 is 1.0 ms on the sample, 7.0 ms on a 1e3 store (an 8.8 KB definition), 3.5 ms on 1e4 and 2.4–3.5 ms on 1e5. The target was under 50 ms. Store size doesn't matter. Text length and the number of dependants do, at about 10 µs per dependant. The adapter adds microseconds over Sophia's `Interface`. A cold first save takes 8–19 ms with the store file in the page cache and up to 0.65 s without. The change feed's 500 ms poll is the one large number. It only delays edits made by another client, because a save from this workspace refreshes the graph itself, so it stays for now. A push feed (ADR-0015 allows one) would replace it if two-client editing needs it.

## Desktop check (2026-10-21)

Release desktop build with `--features desktop,sophia` on a private headless display (`tools/headless.sh`), against copies of Sophia's B-15 stores (`gen1000`, `gen100000`) and its LinearAlgebra ingest (`la`, Sophia 202e137a).

- **Works:** opening a store from the path box; the `views` folder and its tables in both dialects; opening a name, saving an edit (new token), a parse error (`5:13: expected `end``, buffer kept), fixing and saving again; a rendering-profile switch reprinting the open name without a reload (after the fix in `reload_clean`); two app processes on one store, where a save in one shows in the other within a second and an edit made meanwhile conflicts instead of overwriting.
- **Fixed on the way:** open documents didn't follow the change feed (Milestone 16's planned gap), and the Explorer rendered every child, so a 100 000-name namespace lost its first 65 536 rows (now pages of 1000).
- **Timings** (`examples/browse.rs`, release, warm page cache): on Sophia 4c2aaab3 the root of `gen100000` lists in 0.3–1 ms and its 100 000-name namespace in 240 ms; the graph panel's `Query::All` takes 0.5 s for 100 000 nodes (50 MB as JSON) and loads by itself when the store opens; neighbours and a text take under a millisecond. The adapter adds about 110 ms per 100 000 nodes over Sophia's `Interface` (converting and remembering ids). The first check's 0.9 s and 3.3 s were a cold page cache on the 680 MB store file.
- **Sophia's side, fixed in 4c2aaab3:** Julia records of an ingest print their source text (a store ingested before then needs re-ingesting; all 945 LinearAlgebra names print), listing skips sub-namespaces, a parse error names the token as written, and the Rust profile prints `let` and `match` with braces. Still open there: about 7% of `sophia gen` definitions nest deeper than the parser's limit and can't be saved back unchanged (SP-015). A Julia definition saves through the sophialang parser; there is no Julia write path.
- **The graph** (on Daniel's desktop, after P-222's fix, 2026-10-22): the panel drew nothing at first, for folders too; that was the desktop app's WebKit setup, not the store (P-222). Fixed, LinearAlgebra (5106 nodes, 6915 edges) and `gen100000` draw; the 100 000 nodes take about 15 s to appear (P-224), then zooming, panning and clicking a node (opens its editor) stay smooth. Zoomed far out a drag moves a node instead of panning (P-223).

## Limits

- Conflicts are reported with the file wording ("The file changed outside the editor").
- No annotation edits (SPEC-004 §3: a later op), no `datalog`, no typings yet (Sophia M4).
- Polling only.

## P-224: large graph queries (2026-10-22)

The adapter translates and remembers each returned node once, then resolves
edge endpoints through that per-result key-to-id map. It no longer computes and
remembers both endpoint identities again for every edge. Kind-filtered and
missing endpoints are dropped before copying edge properties; ids, order within
one result, source key resolution and properties are preserved. The acceptance
suite compares the filtered graph against the corresponding full graph.
`MOONKALE_GRAPH_PROFILE=1` logs only numeric interface/adapter durations for
`Query::All`; renderer/transport timings live in `editors/graph/graph.md`.
Development builds optimize this query path's Sophia and SQLite crates;
release builds already do so. See graph-render.md for end-to-end evidence.

The watcher's initial `changes_since(0)` now reads `store().last_seq()` once,
instead of decoding every historical change in 256-entry pages. The contract
is unchanged: the first answer carries only the tail position, with no paths
or reset; later polling still consumes bounded pages. This removes work
proportional to historical log size and SQLite contention at store opening.

## WP-06 coarse namespace queries / P-229

`Query::Coarse` enumerates namespace/name/definition-ID/indexed-reference
metadata on the blocking worker. It does not decode terms to infer Calls. A
shared content definition belongs to its smallest stable name id for navigation;
all original Names relations remain. Definition labels in these navigation
snapshots are short content ids; normal Node/fetch APIs retain their full detail.
The adapter checks the log position before/after listing and refuses changes.
Bounds and projection are shared with core.

The copied P-224 store has 199,257 graph nodes and projects to two representatives
in 3.68 s cold / 1.81–1.83 s warm. The initial full-snapshot implementation
exhausted its blocking worker stack; metadata listing avoids that path. Original
full graph/neighbour/body traversal remains an upstream follow-up (P-229).
