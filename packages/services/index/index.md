---
title: "index — implementation notes"
tags: [crate-notes, milestone-2]
---
Notes for `moonkale-index` (Milestone 2). Design: [[Indexing]].

- `graph.rs` — `IndexGraph`: mirrored folder nodes + derived nodes; edges keyed by the file they came from (`set_derived(origin, nodes, edges)` replaces a file's contribution atomically); `by_stem`/`by_path` maps for link resolution; `neighbours` (BFS with direction), `all` (kind filter, cap, files first), `children`.
- `extract/wikilinks.rs` — `[[Name]]`, `[[Name#h|alias]]`, `[[dir/Name]]`, `[text](rel.md)`; Obsidian resolution (stem, case-insensitive, shortest path wins); unresolved targets become phantom `Page` nodes owned by the index.
- `extract/symbols_rust.rs` — tree-sitter: fn/struct/enum/trait/mod/impl/const/static/type/union/macro, nested items via `body`, `Defines` from the file and `Contains` from the parent item. Keys: see *Stable symbol keys* below (they included the line number until 2026-10-11).
- `walk.rs` — BFS over `Query::Children`, skips `.git`/`target`/`node_modules`, caps files and text size.
- `source.rs` — `IndexSource::build(folder)`; id `index:<folder id>`; root = the folder's root; `fetch_text` delegates to the folder for mirrored nodes; `refresh(node)` re-extracts one file; `descriptor().display_name` carries the stats.
- Native only (tree-sitter's C runtime); an empty crate on wasm32.

Tests: `cargo test -p moonkale-index` (4 integration on a tempdir vault + 2 unit).

## Milestone 4: search
`search.rs` — `SearchIndex`: text files are split into ~40-line chunks (cut at blank lines), tokenised (lowercase words; `snake_case`/`camelCase` parts as well as the whole identifier), ranked with **BM25**; when chunks carry embeddings and a query vector is available, a cosine ranking is fused in by reciprocal-rank fusion. `IndexSource::build_with(folder, embedder)` accepts a `moonkale_llm::Provider`; `embed_pending()` fills vectors in batches (desktop and server call it in the background after open, so search is BM25-only until it finishes) and `refresh` re-chunks one file. Exposed as `Query::Text { dialect: "search", text }` → a table (`path, line, score, snippet`) plus the matching `File` nodes — so `RemoteSource` carries it to the web client unchanged. Used by the Search panel (`shell/src/search.rs`, Ctrl+Shift+F) and the `index.search` tool.

## Milestone 7
`refresh(node)` handles the three new cases: a vanished node (rename/delete) drops its subtree, derived data and `Contains` edges and re-extracts the files that linked to it (their links become phantoms); a new directory is walked; a new file gets its parent's `Contains` edge and the files whose phantom links it resolves are re-extracted. `IndexGraph::{subtree, remove_subtree, origins_linking_to, phantoms_for, add_edge}`. Test `refresh_follows_rename_delete_and_new_directories`.

- `extract/wikilinks.rs` (2026-09-20): markdown links whose target contains `://` are external and produce no phantom page.

## Spec 022 (2026-09-21)
- `extract/symbols_julia.rs` (`tree-sitter-julia` 0.23): `function` (long and short form `f(x) = …`, dotted `Base.show` kept), `struct`/`mutable struct`, `abstract type`, `primitive type`, `module`, `macro`, `const`; the grammar has almost no named fields, so `head_name` reduces `signature → call_expression → identifier` through `where`/`::`/`{…}`; definitions inside a `module` get `Contains`. `extract/symbols_python.rs` (`tree-sitter-python` 0.25): `def`/`class` (through `decorated_definition`), methods `Contains`ed by their class. Dispatch in `source.rs::extract_into` by `language_hint` (`julia`, `python`). Neither extracts calls or references — nor does Rust's.

## Milestone 14
The tree-sitter runtime is `arborium-tree-sitter` and the grammars `arborium-rust/-julia/-python` (P-113: one `links = "tree-sitter"` per binary, shared with the Rust code editor). Same `Parser`/`Node` API and node kinds; the three extractors changed two lines each and their tests pass unchanged.

## Stable symbol keys (2026-10-11)
A symbol's key used to end in `@<line>`, so its id changed whenever a line was inserted above it: the graph view lost the node's position and pin, and nothing could refer to a symbol across an edit (raised in the review of the editor-extensibility RFC). All three extractors now build their nodes through `extract::Symbols`. The key is the definition's path: `"<file>#<kind>:<name>"`, nested ones `"<parent key>/<kind>:<name>"` (`src/lib.rs#impl:A/fn:new`). The n-th repeat of a key in one file gets `~n` (`impl:A~2`, Julia methods `function:f~2`). An id now changes only when the symbol is renamed, moves to another parent, or gets a same-named sibling before it. The line moved to the `line` property (`Value::Int`, 1-based). Nothing parsed the line out of the key: the graph view opens a symbol's file through its `Defines` edge. Tests `extract::tests::{keys_are_paths_not_positions, ids_survive_lines_inserted_above, julia_methods_are_numbered_in_order}`.

## WP-65 / WP-22 (2026-10-15)

`extract::Symbols` now emits `Custom("Definition")`, `Name` and (for Rust) `rust:Impl` containers, with shared constants from core. `Defines` occurrences contain UTF-8 byte/line ranges and tree child paths; `Contains` preserves nesting and `Names` connects namespace nodes. Julia keys include signatures, so moving unlike overloads does not renumber methods. Exact-file provenance is shared with wiki-link extraction. Definition nodes keep legacy labels and `line`; graph navigation still opens the incoming occurrence file, and search still indexes file text. The outline uses its separate LSP/editor contract.

`extract/references.rs` walks the same syntax tree without executing code, resolves conservative same-file bare-name targets, and records `Calls`, `References`, `Imports` and explicit unresolved uses. Calls/reference edges aggregate count and ranges. Per-file refresh removes all old derived contributions, including shared-within-file overload names and unresolved nodes. Graph filters keep definitions and impls under symbols and show unresolved code targets under unresolved when symbols are on. Legacy `Symbol` filters read definitions. Full schema coverage (fields, variants, generic containers, module-level bindings/bodies), cross-file semantics, full index persistence and printing are separate work; default-off content identities are described below; see [[Code Graph Schema]]'s implementation contract.

Tests cover the three-language file fixture, body-edit identity and freshness, Julia overload reordering, ambiguity/local shadows, repeated-call ranges and legacy filters, alongside the existing link/search/refresh suites.

## WP-66: explicit content identity requests (2026-10-16)

The optional, default-off `content-ids` feature adds `content::{ContentIdentity, ContentIdError}` and `IndexSource::content_id(definition)`. Existing constructors and the Source protocol are unchanged. A successful explicit request publishes the identity and full digest as definition properties; an ordinary index build computes none. `with_content_id_store(store)` attaches an optional rebuildable cache over `moonkale-state`, without depending on ext-api or a database engine. Existing app constructors retain no persistent content cache and no UI enables this feature.

The normal form and hashing live in the crate `moonkale-content-id` (`packages/services/content-id`, 2026-10-20), shared with Sophia. Its `definition` serializes versioned language syntax, normalizes supported lexical bindings and excludes the declaration name/docs/trivia. Its `identities` hashes reachable same-file dependency SCCs canonically, preserving ordered reference ports and treating symmetric members as identical. `content/mod.rs` resolves the reference spans it needs with the extractors (Rust, Python). For Julia, `source.rs` reads the definition's whole module, which is its package from `Project.toml` when the include graph reaches the file and otherwise the file alone, and the crate resolves it (v5); the cache key hashes every file of the module. `content/cache.rs` bounds memory/persistent LRU entries, validates checksums/version/identity metadata and treats errors as misses. `source.rs` moves computation/store work to a blocking worker and guards publication with exact input hashes, index generation and node existence. Refresh clears the file's published identities.

The full language-specific observable-spelling rules, conservative unsupported syntax, encoding, recursion tie rule, budgets and stale-snapshot limits are specified in [[Code Graph Schema]] §WP-66. These identities are experimental syntactic fingerprints, not semantic proofs or authorization identities. Core id encodings remain unchanged. Full persisted indexing and cross-file/compiler resolution remain separate work.

Run `cargo test -p moonkale-index --features content-ids --locked` and the matching all-target warnings-denied Clippy command. Feature-off workspace tests remain required. The new production dependency is optional `blake3` 1.8.7 (exact lockfile pin); serde, serde_json and the workspace state contract were already present elsewhere.

Example for a native consumer (the caller already has a definition location id):

```rust,ignore
let index = IndexSource::build(folder).await?
    .with_content_id_store(store); // optional Arc<dyn StateStore>
let identity = index.content_id(definition_id).await?;
// identity.id is the UUID; identity.digest retains the full 32-byte hash.
```

CI runs the feature-enabled index tests and Clippy in addition to feature-off workspace gates. `tools/check-content-id-deps.py` generates an isolated target-only manifest from the workspace lockfile and checks BLAKE3's graph with pinned cargo-deny 0.20.2 (advisories, licenses, bans and sources), verifying that every audited crate version/checksum matches production. This is scoped to the new hashing dependency; the broader workspace supply-chain policy remains WP-47.

## WP-67 slice 1: lossless Rust syntax (2026-10-16)

The separate default-off `lossless-syntax` feature exports `lossless::{RustSyntaxSnapshot, RustEditPlan, RustTextEdit, RustFunction, SyntaxToken, SyntaxEditError}`. It reuses optional BLAKE3 and needs no new dependency. Snapshots preserve exact Rust bytes/tree/token/trivia partitions; pure declaration-name and same-file top-level function-move plans validate exact inputs, byte ranges, grammar and move token conservation. `apply_to` returns a proposed String, without touching documents or sources. Resource limits and the explicit trivia ownership policy are in [[Rust Lossless Syntax and Patches]]. The planner itself stays pure. WP-67 slices 2–3 add a separate revision-aware ext-api document adapter and opt-in desktop/web graph preview/conflict controls (`structural-edits`); reference-aware rename and cross-file moves remain unimplemented. Existing constructors now supply the core SourceTruth default; remote descriptor preservation remains the remote layer's responsibility.

`cargo test -p moonkale-index --features content-ids,lossless-syntax --locked` exercises both opt-in features. CI uses the same feature combination. Neither is an application default.

## Text read failures (WP-23, 2026-10-17)

Build, refresh and dependent re-extraction publish `index_error = "text read failed"` on the mirrored file node when `fetch_text` fails, including non-UTF-8 input. This is a content-free node diagnostic, accessible through `Query::Node` and graph queries; no UI string or lossy decoding is introduced. Failed reads clear stale search chunks and derived symbols/links. A subsequent successful read clears the property and resumes indexing. Files excluded by the normal size/type limits are still intentionally skipped. `invalid_utf8_is_reported_and_recovery_clears_the_diagnostic` tests error publication and recovery.

SR-145 (2026-10-24): shared content-id parsing/normalization no longer refuses
nesting beyond 128. Explicit heap stacks and total byte/work budgets replace the
depth guard, preserving normalizer v7 and golden ids. The index regression now
expects a 140-parenthesis body to receive an id; source fetch, reachable-definition
and cache limits remain enforced independently. The shared crate's
`tests/deep.rs` covers depth 100,000 on 256 KiB stacks.

## WP-21: derived folder store (2026-10-24)

`sophia` adds `folder_store/{mod,producer,digest}.rs` (default in apps/server since ADR-0030). The index remains
in memory and keeps its current queries. Desktop, server and mobile builds
with that feature register `.moonkale/code.sophia` as an additional graph source.
The wasm client uses the server's `RemoteSource`.

The writer uses the bounded, ignore-aware folder walk: Rust, Python and Julia
only, at most 512 KiB per file, 5,000 walked files and 64 MiB aggregate code
text. A truncated walk fails visibly instead of deleting unseen contributions.
Markdown, configuration, hidden files and ignored files never enter this store.
The cache directory has its own `*` ignore file and is explicitly skipped by
the index walk. Cache paths may not be symlinks.

`CodeFile` is the admitted snapshot, `FolderStore::sync` the blocking writer,
and `FolderReport` the evidence (parsed/skipped files, writes, retries, refusals,
rebuild). Reads/hashing occur on reopen; unchanged files cause zero parses and
zero `replace_files` calls. A changed Rust/Python file uses the index extractor's
lexical references on its content-id tree. Julia follows literal includes in
module mode, records dynamic includes conservatively, and re-evaluates module
dependants; only changed manifests are replaced. Refused definitions contribute
no guessed identities; ambiguous named bindings (for example conditional Rust
statics) keep their records but publish no arbitrarily chosen name, and appear
in `FolderReport.naming_diagnostics` and index-root properties; aggregate/budget/conflict errors leave the prior store
and report an error through `folder_store_report`; refresh keeps the current
in-memory graph available and the code graph visibly falls back.

The record mapping **mirrors** Sophia `sophia-ingest/src/code.rs::to_record`
at `c3c48e02`: canonical bytes contain full referenced digests, `refs` excludes
intra-component references, and display/kind/component metadata come from shared
content-id output. Sophia does no parsing. Names use shared `binding_groups` /
`entity_path`, with nearest Cargo package names (hyphens become underscores),
conventional Rust file modules, Python `__init__.py` / `pyproject.toml` roots,
and Julia include bases and module ownership. Unknown Rust/Python roots use the
shared path-derived `unknown` namespace. No normalizer or content-id changes.
Generic-call facts have kind `uses_generic`, producer `moonkale-index`, version
`moonkale-wp66@7`, and a UTF-8 absolute generic path as value. Paths are resolved
when read: nominal ids are allocated within Sophia's transaction.

`Source::refresh_batch` preserves a watcher debounce window through workspace,
server function and remote proxy. The writer reconciles the current snapshot
once, including old-path deletion/new-path insertion and Julia dependants, in
one `replace_files` transaction. A structured `NameClaimed` adds its owner from
the admitted snapshot and retries at most 16 new owners; empty/repeated owners
fail. Sophia owns fact replacement and rollback (manifest format @1).
A different `folder/normaliser` rebuilds the cache; deleting it is safe.

Acceptance tests: `cargo test -p moonkale-index --features sophia --test folder_store`.
The `folder_store` example measures a real folder without a UI. See [[Indexing]]
for the repository measurement and limitations.

### History replay (ADR-0029 gate 4)

`cargo run -p moonkale-index --features sophia --example history_replay --
REPO RANGE SCRATCH_DIRECTORY` is an opt-in measurement, with no UI or server.
Create the scratch directory **outside** the input repository first. The tool
canonicalizes both paths and refuses scratch locations inside the repository.
Git is read through `rev-list`, `ls-tree` and a persistent `cat-file --batch`
process. It never checks out, adds a worktree to, or changes the input repo.
Scratch snapshots and databases are temporary and removed on normal exit. With `MOONKALE_REPLAY_KEEP=<dir>`, the final replayed and fresh stores are
copied there first (`replayed.sqlite`, `fresh.sqlite`).

Commits run oldest first in reverse topological order. Each transition between
adjacent trees becomes one selected debounce window passed to
`FolderStore::sync_selected`, including rename old/new paths. Non-root ranges
first build the first commit's parent as a baseline. Only bounded regular
Rust/Julia/Python blobs plus Cargo/Python naming and ignore metadata are
materialized; tracked symlinks, internal cache/build directories and other
files are skipped. The ordinary ignore-aware FolderSource walk then selects
inputs. Aggregate/parser limits remain in force. This measures the folder
writer's watcher batch path, not OS notification delivery or full index/UI
startup. Non-code changes are not counted in `changed_paths`.

JSONL reports each batch's time (writer only), selected-path and replaced-file
counts, retries, SQLite size, retained record count, live fact count and errors.
Every 50 commits and at the end, an independent scratch tree is built afresh.
Comparison covers manifests, actual names, content identities/bytes, text-source
pointers, live generic memberships (including orphan relations), and actual
facts even on historical record subjects. Append-only records/logs, random
nominal UUIDs and first-writer display metadata are not equivalence criteria.
Each difference is reported with the commit and owning paths when available.
Errors/findings cause a nonzero exit after the report is written; no repair is
attempted. Query timing failures are recorded separately in checkpoint output.

Checkpoint reads use 100 name lookups, 100 fixed-seed random definition
`text_source` reads with replacement, ten `Source::query(All { limit: 1000 })`
graph reads and ten actual Search-panel `Text { dialect: "search", text:
"main" }` reads. Empty stores have no lookup/text samples. Reports contain
sample counts, median and nearest-rank p95. Sophia currently rejects the search
dialect; those durations measure rejection, not successful searches.

`cargo test -p moonkale-index --features sophia --test history_replay` runs the
hermetic five-commit add/modify/rename/move/delete replay in CI, with checkpoints
at every commit and a second non-root range. It also checks scratch refusal and
that input Git status is unchanged. The P-231 regression asserts the upstream
contract; it failed on Sophia v0.1.0 and passes since the fix (`77f5c1c9`). Full history measurements and findings live in
[[Indexing]].

### Default legacy reads (ADR-0030, 2026-10-26)

Desktop/mobile enable `sophia` by default; building the server implies it.
`IndexSource::set_cache_reader` attaches a read-only `Arc<dyn Source>` after the
writer syncs, without leaking Sophia types. Ordinary index/search queries keep
their prior meaning. The graph uses a bounded `code-graph` text envelope carrying
`(force_memory, Query)`: a per-request choice, safe for separate server clients.
`code-graph-status` returns an empty reason when the cache is healthy, otherwise
a fallback reason. File/note nodes and wiki-links stay index-owned; code-file
Local queries use the adapter's bounded `folder-neighbours` manifest path.

A cache open/sync/read failure never prevents folder indexing. A broken database
gets one rebuild attempt per index lifetime; readable authored stores and all
cache symlink paths are protected. `check-code-cache` builds a new producer
snapshot in a temporary directory under `.moonkale`, compares the shared
`folder_store::digest` full-digest-v1 state and repairs a mismatch. It returns
`same` or differing keys and logs the problem without source bodies. The shell
owns translated status items, the persisted `code_graph.source` setting and the
**Check the cache** palette command.
