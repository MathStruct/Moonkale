---
title: "Rust Lossless Syntax and Patches"
description: "WP-67 foundation: source truth metadata, exact Rust syntax snapshots and guarded pure patch plans; revision-aware proposals and opt-in desktop/web graph controls with preview/conflict handling are built."
tags: [architecture, code-graph, rust, syntax, patches]
---
**Built, experimental and off by default, 2026-10-16:** WP-67 slices 1–4. Implements the foundation of [[ADR-0028 Files or the graph]] option B through opt-in graph actions. See [[Code Graph Schema]], [[Indexing]] and [[P-202 Lossless edits must conserve syntax tokens]].

## Source truth is metadata, not permission

`SourceDescriptor.truth: SourceTruth` declares `Files` or `Store`, serialized as `"files"`/`"store"`. Missing fields on old descriptors deserialize to `Files`. Existing source constructors explicitly retain that default, including databases whose external source remains authoritative. Remote descriptors preserve the received value. `Store` grants no write permission; `capabilities.write` and the server's ordinary authority still apply. No existing source is converted to graph-owned storage. Rust callers constructing descriptors must add `truth: Default::default()`; the library changelog records this breaking struct-literal addition, and no tag is cut here.

## Exact syntax snapshot

Native `moonkale-index/lossless-syntax` is a separate default-off Cargo feature. It reuses the already-added optional BLAKE3 dependency, independently of `content-ids`. `RustSyntaxSnapshot::parse(&str)` owns exact source bytes, the tree-sitter tree, full input hash, token ranges, leading-trivia ranges, EOF trivia and top-level function metadata. Comments are trivia rather than lost text. No whitespace, CRLF, BOM, literal spelling or Unicode normalization occurs. `print_original()` reconstructs bytes from token/trivia partitions, not a pretty-printer.

Every source byte belongs to a token or a trivia gap. Trivia precedes its next token; the final gap belongs to EOF. Function move ownership starts at the previous non-comment top-level item's end (or after the protected file prefix), so adjacent leading comments and blank lines travel with the function. The syntax range already contains attached doc comments/attributes. A same-line trailing comment and its line terminator also travel. File BOM, shebang and inner attributes stay protected; standalone final comments/whitespace remain file-owned EOF trivia. This is an explicit v1 ownership policy, not an inferred statement about which prose the author intended to move.

The snapshot is an explicit owned working value, not a global cache or persisted copy. Callers schedule parsing outside the UI input path and release snapshots when no longer needed. Input/output are capped at 512 KiB; tree walking is capped at 200,000 nodes and depth 128, with a two-second parser progress deadline. Parsing errors or limit exhaustion are typed failures.

## Pure plans

```rust,ignore
use moonkale_index::lossless::RustSyntaxSnapshot;
let syntax = RustSyntaxSnapshot::parse(text)?;
let rename = syntax.plan_declaration_rename(0, "new_name")?;
let proposed = rename.apply_to(text)?;
let move_before_first = syntax.plan_move_function(1, Some(0))?;
```

- **Declaration rename** changes exactly the selected top-level function's name token. It does **not** update calls, imports, macro text or other files, and must not be presented as semantic Rename. Names are checked with a parser probe and the Rust Reference's strict/reserved/raw restrictions; the conservative policy rejects reserved words from every edition. Source spelling and normalization are retained. Name requests are at most 256 UTF-8 bytes. See [keywords](https://doc.rust-lang.org/reference/keywords.html) and [identifiers](https://doc.rust-lang.org/reference/identifiers.html).
- **Move function** reorders a top-level function before another function's owned range, or before file-owned EOF trivia with `before=None`. It produces a deletion and insertion against the original byte coordinates, without adding separators or formatting. No-ops return an empty plan. Cross-file moves, nested item moves, reference updates and compiler semantic checks are not implemented.
- `RustEditPlan` exposes read-only `input_hash()` and `edits()`. Each `RustTextEdit` has a UTF-8 byte range, expected removed text and inserted text. Its `apply_to(current)` checks the exact input hash, all boundaries, expected text, overlap and final size before constructing output right-to-left. Stale text is rejected, not rebased automatically. Output limits count the final size, so an equal-size move at the cap is valid.
- Planned output is reparsed and must retain the function count. Moves additionally preserve the complete multiset of terminal token kind/spelling pairs. Grammar validity alone is insufficient: an unterminated EOF line comment can swallow another item while leaving a valid parse. Such placements return `InvalidPlacement`. No unsafe partial plan is returned.

Neither planning nor `apply_to` writes a file, edits a Workspace buffer, changes revisions/history, saves or refreshes the index. The returned String is a proposed result; the explicit document adapter below performs the separate in-memory commit. `SyntaxEditError` distinguishes invalid names/functions/ranges, overlap, stale input, invalid placement, malformed syntax and limits.

## Revision-aware document adapter

`Workspace::document_edit_base(node)` captures an already-open document's unsaved text, editor revision, source version, generational document/session handles and source instance. It supports writable `Files` sources only; Store ownership is not implemented by this adapter. A stale editor mirror returns `Changed` instead of overwriting the canonical buffer. Capture on the UI thread, clone the snapshot text for background syntax planning, then return byte edits to the UI thread:

```rust,ignore
let base = ws.document_edit_base(node)?;
let syntax = RustSyntaxSnapshot::parse(&base.snapshot().text)?;
let plan = syntax.plan_declaration_rename(0, "new_name")?;
plan.apply_to(&base.snapshot().text)?; // verify the syntax plan's exact input
let proposal = base.propose(plan.edits().iter().map(|edit| TextChange {
    range: edit.range.clone(),
    removed_text: edit.removed_text.clone(),
    inserted_text: edit.inserted_text.clone(),
}).collect())?;
// Present proposal.base().text and proposal.preview() as plain data.
let committed = proposal.apply(ws)?; // no save
```

`workspace::proposals` is engine-neutral; it depends on the existing `TextChange`/`EditBatch` contract, not the index/parser. A native-only development dependency tests this exact bridge with real Rust plans. Normal builds and wasm do not acquire the parser dependency through ext-api.

`propose` validates ranges, expected removed bytes, UTF-8 and overlap in a disposable editor session before exposing a preview. Base/output are capped at 512 KiB, edits at 4,096, and total removed/inserted payload at 1 MiB. `apply` rechecks source instance and Files/write capability, document/session identity, source version, editor revision, canonical text and editor text before a synchronous commit. Even a close/reopen or edit/undo returning identical bytes cannot accept the old proposal. Invalid or conflicting application changes no buffer/history. A successful edit is one ordinary undo entry, retains current live selection rebased in UTF-16, and marks the document dirty through the existing text/saved comparison. A no-op creates no history entry. Other editor views observe the same session and canonical document.

Conflicts are typed (`Closed`, `UnsupportedSource`, `Changed`, `LimitExceeded`, `Invalid`). `apply(&self, ...)` retains the base and preview on failure: a consumer can compare those with the current document, discard the proposal by dropping it, or capture a new base and rerun the planner. Old ranges are never automatically rebased. The backing source is not fetched or written during application; external file changes remain the responsibility of the ordinary version-checked `Workspace::save`. Its `SourceError::Conflict` leaves the proposed buffer dirty and undoable. The optional graph UI below consumes this contract.

## Evidence and the next slice

Tests cover byte-exact Rust corpus reconstruction (including real project source), Unicode/CRLF name-token edits, leading docs/attributes and trailing comment ownership, file-prefix protection, no-ops, stale input, invalid UTF-8 and overlapping edits, invalid names, deep/oversized inputs, a near-limit backward move, and EOF-comment token loss. Descriptor tests cover legacy JSON defaults, both values and remote Store preservation without write capability.

CI runs both experimental index features alongside the feature-off workspace checks. The final handoff records the exact gate/browser evidence. No new dependency or graph-render bundle change is introduced by this slice.

**WP-67 remains conservative and experimental.** Graph controls and explicit conflict presentation are now implemented behind `structural-edits`; reference-aware rename now uses complete single-file rust-analyzer plans; multi-file rename and cross-file move remain unimplemented. Cross-file work needs an explicit multi-document failure/undo policy because Source transactions permit partial success. No autosave, mode/default flip, multi-file atomicity or semantic-equivalence guarantee is claimed here.

Document-adapter integration tests additionally cover real Rust declaration edits and moves, exact Unicode/CRLF output, live selection, one-step undo/redo, revision ABA, close/reopen ABA, source replacement, permission/truth changes, canonical-buffer drift, source-version changes, malformed/overlapping edits, bounds, no-op history and a failed save followed by explicit recomputation. See [[P-204 Structural proposals must retain document identity]].

## Experimental graph controls (slice 3)

Build desktop or web with `--features structural-edits` (web's server and wasm client must both enable it). The feature is forwarded through distribution to the graph panel and through the app to the planner service. Default builds expose neither controls nor planner endpoint. No renderer bundle changes or new default setting are required; dragging remains graph layout only.

Select one Rust function definition in the graph and choose **Edit Rust function (experimental)**. The panel offers **Change declaration name**, **Rename with references (single file)** and **Move function**, with a same-file **Move before** selector or **End of file**. The declaration-name action changes only a declaration token; the separate Rename action uses the language server. The source file is resolved using the selected node's own index and Defines edge, then loaded in the background without replacing unsaved edits. Old index provenance/ranges preselect a function only when they match the captured input; otherwise choose a function from the freshly parsed snapshot. Only top-level functions returned by the planner can be edited.

Preview renders original/proposed text as plain escaped source with exact replacement ranges highlighted. Apply uses the existing proposal adapter, creates one undo entry and leaves the buffer unsaved. Open its normal source tab to undo/redo or save. Discard drops review state without changing a document. Conflict retains original/proposed text alongside current text, disables Apply and requires **Recompute** followed by an explicit fresh function/destination choice and another preview. Completed review state survives panel remounts in Workspace-owned signals. Selection changes or dismissal invalidate pending work; snapshot validation rejects responses after document edits, session replacement or closure.

`ext-api::structural` defines the pure protocol and optional StructuralPlanner service. Function handles include the entire input digest and syntax range; they cannot be reused on another snapshot. Desktop uses local blocking workers; web uses POST `/api/structural/plan` behind the existing auth/origin gate. The endpoint is content-only: no paths, source lookup, code execution or file writes. The auth middleware caps the request body at 4 MiB before deserialization; decoded source is at most 512 KiB, functions/edits at most 4096, combined edit payload at most 1 MiB, and at most two parser jobs run per process with immediate Busy rejection. Existing tree-sitter size/depth/work/time limits apply. UI errors are localized categories rather than raw source/parser output.

See [[P-205 Graph previews must reject stale asynchronous results]]. The browser suite `structural-edits` requires the feature-enabled server/client build and exercises the real graph renderer, remote planner and native editor together.

CodeMirror retains its own undo stack. When it emits a change, any existing Rust
revision mirror adopts the canonical text, invalidating outstanding proposals;
an unchanged echo of an applied Workspace edit preserves the Rust history.
Both backends are exercised by the structural browser workflow. Save completion
records exactly the submitted snapshot, so later edits remain dirty while the
source write is pending ([[P-206 Save completion must acknowledge submitted text]]).
The Graph review remains visible after attaching a previously unopened source tab.

CodeMirror Workspace replacements use isolated undo boundaries and widen their
common-prefix/suffix diff over complete UTF-16 surrogate pairs and CRLF.
Protocol probes cover both surrogate boundaries, and the graph workflow renames
supplementary-plane Rust identifiers end to end ([[P-207 CodeMirror external edits must preserve UTF-16 boundaries]]).

## Reference-aware graph rename (slice 4)

**Rename with references (single file)** uses the existing shared rust-analyzer
session, on desktop and web, against the selected function's exact declaration
name byte offset. The lossless planner first validates the snapshot-bound target
and requested name. The code-view component converts the offset to UTF-16,
synchronizes open unsaved buffers, retains the source document's LSP lease, and
requests `textDocument/rename`. Startup retains the manager's existing 30-second
timeout; the rename request has a five-second timeout. No new server endpoint,
compiler launcher, dependency or graph renderer asset is introduced.

Every returned URI and document version is checked before constructing a preview.
If **any** returned target requires another file, the **whole plan is refused**;
local edits are never extracted from a multi-file rename. Malformed/resource
operations are rejected by the existing checked LSP decoder. URI aliases share
identity, paths must stay within the originating folder, edits are bounded to
4096 / 1 MiB payload and the document adapter's 512 KiB input/output. UTF-16 ranges
are converted through the existing validated Unicode/CRLF path; overlap and
invalid boundaries are rejected. Empty plans are reported without an Apply button.

The origin's source/document/session identity, version and revision are rechecked
after startup and response. Open document handles, text and editor revisions,
along with the originating language-server connection and document version, must
remain unchanged during the request and are retained as a guard checked again before Apply. The resulting original/proposed preview,
conflict retention, fresh recomputation, discard, grouped undo and separate save
use the same path as other graph proposals. This is language-server rename under
its current project configuration, not a proof of completeness across disabled
code, generated code or every possible build. Files not loaded in the client are
resolved by the language server; there is no new disk-wide snapshot transaction.

`graph-rename` runs with both editor backends against a real Rust crate and
rust-analyzer: recursion and unsaved calls change, comments and shadowed locals
stay exact, multi-file plans change nothing, and conflicts retain the preview.
See [[P-208 Graph rename must refuse incomplete file subsets]].
