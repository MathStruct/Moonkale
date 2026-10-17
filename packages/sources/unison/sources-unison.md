# Unison source (WP-18)

`moonkale-sources-unison` implements a read-only `Source` over an immutable local-codebase snapshot. Descriptor: `truth: Store`, family `Graph`, read enabled, write/watch disabled, no text dialect. The crate builds without UCM. Enable the default-off `unison` feature in `desktop`, `web` (both client and server builds), or `moonkale-distribution`; the server has its own matching feature. This is a source driver, not an extension manifest.

## Opening

Install `unison-ucm` (included in the Linux flake dev shell), or set the **operator's** `MOONKALE_UCM`; discovery otherwise uses `PATH`. UCM 1.5.0 is the tested version. The source opener recognizes codebase root directories named `*.unison`, containing `.unison/v2/unison.sqlite3`. A standard codebase root with another name can be opened through `UnisonSource::open(path)`; rename the root directory to `example.unison` to use the application's ordinary source opener. Do not select its internal `.unison` directory.

All projects and their branches are loaded under branch namespace nodes, with nested Unison namespaces as `Directory` nodes. `Children` follows lexical `Contains` only. A name is `Name`, with `Names` to a `Definition`; names shared by a term and a type have separate keys. Definitions are keyed by `NodeId::from_content` of the UTF-8 bytes of the **full canonical Unison reference**, including component/constructor selectors. `content_id` retains that reference. This encoding is Unison-specific, not a WP-66 BLAKE3 identity. Index zero is implicit (`#hash`, never a distinct `#hash.0`); constructor `#d0`/`#a0` syntax and API `#0` normalize to the same reference. Builtins retain `##name` as UCM's intrinsic identities; they have no component digest.

`def_kind` is function/constant (signature arrow or no arrow), type, interface (ability), variant (data/ability constructor), or test. `lang=unison`, `frontend=ucm-http-1.5`. Recursive definitions retain the component digest and member index separately. `References` comes from UCM's dependency endpoint, including dependencies outside the named namespace as boundary definition nodes. Boundary definitions need not have printed content. `TypeOf` is emitted only for an exact single type reference, or a constructor's enclosing type; constituents of a function signature are dependencies, not falsely its complete type.

`fetch_text(definition)` returns cached UCM-printed source. A constructor displays the enclosing declaration. Immutable snapshot fetch versions are zero. Alias/rename changes affect the names, not definition ids. The graph panel automatically selects a newly opened Graph source, includes all store-backed node kinds in Data mode, and opens text-bearing definitions directly. No new shared kinds were needed.

## Why the HTTP API

Use the versioned, structured local UI API rather than scraping human CLI/transcript output or depending on the undocumented SQLite schema. A fresh owned `ucm -c <root> headless --host 127.0.0.1 --no-file-watch` serves the snapshot; the source kills and reaps it on success and every error. No UCM remains running after `open` finishes. It may require UCM's codebase lock and may perform UCM's own maintenance/migrations; Moonkale never submits writes or evaluation commands. Close another UCM if it holds the codebase lock. This first version deliberately has no polling/watch, refresh-in-place, store editing, or query dialect: close/reopen to see external edits.

API routes are `projects`, project `branches`, branch `list`, `getDefinition`, and `getDefinitionDependencies`. Namespace/type/dependency summaries may abbreviate hashes; resolve those through `getDefinition` and refuse ambiguous/missing results instead of hashing an abbreviation. **All definition display requests use hash-only queries.** UCM's named definition-display path evaluates attached documentation; its hash-only path has no name from which to find docs, so bypasses that evaluation. API output is plain text data, never HTML or executable instructions.

References: [UCM's 1.5 API routes](https://github.com/unisonweb/unison/blob/release/1.5.0/unison-share-api/src/Unison/Server/CodebaseServer.hs), [definition display and documentation handling](https://github.com/unisonweb/unison/blob/release/1.5.0/unison-share-api/src/Unison/Server/Local/Definitions.hs), [API payload types](https://github.com/unisonweb/unison/blob/release/1.5.0/unison-share-api/src/Unison/Server/Types.hs).

## Bounds and trust

The executable comes only from process environment/PATH, never workspace settings, source content or a browser parameter. Spawn is a direct argument-vector command, never a shell. A fresh random per-open token protects a loopback-only listener; it is never returned in descriptors/errors/logs. The client disables proxy discovery and redirects; URLs use encoded path/query segments and only this loopback port. No Share, remote API, downloads, transcript execution, or outbound network requests are made by the adapter. `reqwest` uses no TLS/default features because this API is exclusively loopback HTTP; `uuid` supplies the random token, Tokio isolates blocking work, and serde_json parses bounded responses. All are already workspace dependencies.

Limits: 4,096 nodes, 65,536 edges, 64 namespace levels, 16 MiB aggregate successful API response bytes, 16,384 requests, 60 seconds overall API work, five seconds per request, ten seconds startup. serde_json retains its recursion bound. stdout/stderr go to null (no unbounded capture or source/token logging). The owned child is killed and waited on in an RAII guard, including failed/malformed responses. A caller cancelling `open` leaves only its bounded blocking task until that cleanup, not a persistent server. A loopback port is selected then released immediately before spawn; a bind race fails the open rather than connecting to a configurable external endpoint. A changed branch causal hash during collection rejects the snapshot. Branches are checked individually; this is not a cross-branch transactional snapshot. Large codebases fail explicitly; no silent partial publication. A full project/branch snapshot with a patch object is unsupported rather than silently losing that object. Query `All` caps nodes and retains only edges whose endpoints were returned. Neighbour traversal caps depth at 64.

## Tests

```
CARGO_TARGET_DIR=target/codex MOONKALE_UCM=/path/to/ucm cargo test -p moonkale-sources-unison -p moonkale-editor-graph
```

Tests print an explicit `SKIP ... UCM missing` message if the executable is unavailable (`-- --nocapture` shows it). The hermetic transcript creates definitions, a type and constructors, an ability, a test, a recursive pair, rename, alias and dependency updates. `builtins.merge` uses UCM's bundled definitions, not a network library; builtin names are removed after retaining Boolean so the snapshot stays small and UCM's update printer can resolve Boolean. The fixture removes an old alias before editing: UCM otherwise deliberately preserves existing references to a definition that still has that alias. Tests validate immutable fetched source, openers, kinds, contains/names/references/type edges, capped structured queries, and the graph panel's actual serialized renderer payload. No browser or UCM process is retained.

The separate historical `milestone1.mjs` smoke test explicitly switches README.md from Rich to Source before waiting for CodeMirror. That suite is not one of the CI browser suites.

Pre-existing symlinks in `.unison`, `v2` and UCM's SQLite/lock files are refused. This is a pre-spawn check, not OS process confinement: concurrent pathname replacement remains the existing #5 boundary.
