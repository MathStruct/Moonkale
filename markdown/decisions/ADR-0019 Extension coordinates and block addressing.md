---
title: "ADR-0019 — Extension coordinates and block addressing"
tags: [adr, extensions, editors, core]
status: proposed
date: 2026-10-11
---
**Status:** proposed. Written by Claude after reviewing the editor-extensibility RFC (2026-10-11), which asked which decisions are hard to reverse and should be settled while Moonkale is small. Daniel decides. Related: [[ADR-0008 Rust owns the document, JS is a view]], [[ADR-0009 Patches not snapshots]], [[ADR-0013 JSON ABI before components]], [[ADR-0015 The core model]], [[ADR-0017 Markdown live preview on the code editor]].

## Context
Two things every editor extension will bake in, and that are expensive to change once extensions exist: **which unit a text position is in**, and **how a piece of a document (a block) is named** when it is not a whole file.

**Positions.** Moonkale already uses four units:
- UTF-8 bytes: `core::Splice`, `ext-api::editor::EditBatch` (on `codex`), `wiki::WikiSpan`, and the byte ranges pulldown-cmark and tree-sitter return;
- UTF-16 code units: LSP, CodeMirror's splices (`code-view/src/backend`), `Utf16Selection` and `Utf16Position` in `ext-api::editor`, DOM ranges;
- Unicode scalars: editor-core's offsets and the native decorations (`native_decorations.rs`);
- graphemes: the boundaries a replacement must respect (`valid_replacements`).

Each boundary converts. Extensions would add a fifth party, and an extension that guesses the wrong unit is correct on ASCII and wrong on `😀` or `猫`.

**Revisions.** There are two counters, and they mean different things. `core::Version` is the source's version of a node: it changes on save and is used for conflict detection. `DocumentRevision` (on `codex`) is the live editor session's revision: it changes on every keystroke.

**Blocks.** Nothing names a block today. `NodeKind::Block` exists and nothing produces it. Headings and paragraphs are not in the graph. Symbols are, and since `ccb7d90` their native keys are paths (`src/lib.rs#impl:A/fn:new`, `~n` for repeats) that survive edits above them. Before that they ended in `@<line>`. Wiki links already address headings (`[[Page#Heading]]`; `WikiSpan::heading`) but never resolve them. The RFC wants to open a block, show it in several contexts, and embed one block in another. All of that needs a name for a block that survives edits.

## Decision
### 1. One external unit: UTF-8 byte offsets into the text the extension was given
- Every range an extension receives or returns is a half-open `start..end` of **UTF-8 byte offsets** into the snapshot text it received, tagged with that snapshot's **`DocumentRevision`**.
- The host validates every incoming range: within the text, on `str::is_char_boundary`, and for replacements on a grapheme boundary. An invalid range is rejected with a diagnostic, never clamped.
- UTF-16 stays where it is native: LSP, CodeMirror and the DOM. Scalars stay inside editor-core. Conversions live in one module per boundary (`code-view` owns UTF-16↔bytes and scalars↔bytes). Extensions never see either unit.
- Lines and columns are for display only. When an extension needs one, the host gives a 0-based line and a byte column.

Why bytes: they are what Rust `&str` slicing, tree-sitter, pulldown-cmark, `EditBatch` and `WikiSpan` already use. A wasm module receives UTF-8 JSON strings, so bytes are its native unit too. Validation is O(1). UTF-16 would favour only JavaScript, which is not an extension language here (ADR-0013). Scalars make every slice O(n).

### 2. Results are tied to a revision and mapped, never applied blindly
- An extension's output (decorations, widgets, diagnostics) carries the `DocumentRevision` it was computed against.
- If the document has moved on, the host **maps** the ranges through the `EditBatch`es since that revision, or drops the ones an edit touched. It never applies old offsets to new text. This replaces today's rule of dropping every stale batch, which makes asynchronous providers flicker.
- Edits from extensions are `EditBatch`es against a base revision, with today's `BatchRejected` semantics: stale means rejected plus the canonical snapshot.
- Extensions never see `core::Version`. Turning a session into a `core::Transaction` on save is the host's job.

### 3. A block is `(file, anchor)`, and the anchor is text
A block is addressed as the file's `NodeId` plus an **anchor**, resolved against the current text whenever it is used:

| anchor | written as | resolves to | survives |
|---|---|---|---|
| symbol | `#impl:A/fn:new` (the symbol's native key after `#`) | the index's `Symbol` node, then its definition range | edits elsewhere; not a rename |
| heading | `#Decision/Consequences` (heading path; `~n` for the n-th repeat, as for symbols) | the section under that heading | edits elsewhere; not a heading rename |
| block id | `#^theorem-3` (an explicit `^id` marker, Obsidian's syntax) | the paragraph or block carrying the marker | renames and moves within the file |
| range | `start..end@revision` | itself | nothing: **never persisted** |

- The persisted form is a string, `<file key>#<anchor>`. It is the same string a wiki link uses (`[[Page#Heading]]`, `[[Page#^id]]`) and, for symbols, the same as the native key. One syntax names a block in links, in layouts and in the graph.
- Resolution parses the current text, independently per consumer, as the RFC's D1 suggests. An anchor that no longer resolves is shown as **unresolved**. It is never guessed by position.
- Blocks are **not graph nodes**. Headings and paragraphs are not persisted in the index, and `NodeKind::Block` stays unused until a source owns blocks as data. Content ids (`NodeId::from_content`, ADR-0015) are for identity across files and machines, not for paragraphs.

### 4. An occurrence is view state
Where a block is shown (which panel, beside what, in what order) belongs to the layout. It is persisted with the layout in the state store ([[ADR-0014 One store for internal state]]), if at all. It is never written into a source or into the graph, and changing a layout never changes text.

## Consequences
- **Built:** path keys for symbols (`ccb7d90`, the index crate note). Byte ranges already exist in `EditBatch` and `WikiSpan`.
- **To build, when the first consumer needs it:**
  - heading-path and `^id` resolution, shared by the wiki-link extractor and the editors;
  - mapping decoration ranges through edit batches in the native compositor (`native_decorations.rs`);
  - one conversion module per boundary in `code-view`;
  - reveal state keyed by decoration identity instead of its position in a bitmask.
- **Cost of reversal:** low today, since nothing outside the host consumes these units. After the first extension ships it means a versioned ABI change.
- `ext-api::editor`'s UTF-16 `Utf16Selection`/`Utf16Position` stay as the view↔Workspace contract (they match LSP and the browser). They are not part of the extension surface; an extension asking for the selection gets bytes.
- Heading anchors break when a heading is renamed, as in Obsidian. Rename refactoring (like spec 012's rename rewriting for wiki links) can follow later.
- Not decided here: the decoration vocabulary and the provider interface (the RFC's seams 1–2). They should be decided after experiment E1, using the units fixed here.
