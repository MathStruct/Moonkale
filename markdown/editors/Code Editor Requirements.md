---
title: "Code Editor Replacement Requirements"
description: Moonkale's editor interfaces, the CodeMirror 6 feature surface in use, and requirements for a pure-Rust replacement.
tags: [editors, code, requirements, architecture]
---
These requirements describe the replacement for Moonkale's default CodeMirror-backed code view. They are based on the current Rust/TypeScript implementation and the CodeMirror 6 documentation, checked 2026-10-03. They are a behavior and integration target, not a library selection or implementation design.

Related notes: [[Code Editor]], [[Code Editor Implementations]], [[JS Interop Boundary]], [[LSP and Terminal]], [[Graph-Native Model]], [[Core Languages]], and specifications [[009]], [[010]], [[012]], [[014]], [[016]], [[018]], [[024]], [[025]].

Delivery sequence, first-milestone implementation steps and proposed test cases: [[Rust Code Editor Roadmap]].

## Scope and terminology

“Editor” here means the interactive text editing surface inside `moonkale-code-view`, currently implemented by the CodeMirror 6 bundle. It does not mean the whole Moonkale application, a language server, or the Markdown WYSIWYG view. `moonkale-code-view::CodeEditorPanel` is shared by the generic Code extension and Markdown's Source mode; replacing its editor therefore affects both. Markdown's Rich mode is a separate Milkdown view.

The existing opt-in `moonkale-editor-code-native` extension is a separate editor implementation. It proves that a Rust/Dioxus editing surface can be embedded, but it currently lacks most CodeMirror integrations (notably LSP, folding, search/replace, wiki-link marks, presence gutter and live wrap). It is not yet a feature-equivalent replacement. Whether to promote/fold it into the default editor or replace it with another Rust engine is an implementation decision.

“Pure Rust” means the editor engine, editing model, syntax processing, and Moonkale integration are implemented in Rust, with no CodeMirror runtime/bundle or JavaScript editor engine. Dioxus may still render its normal web/desktop UI. This requirement does not imply removing unrelated JavaScript views elsewhere in Moonkale.

## Moonkale's interface and ownership model

### Host owns the document

The document is opened and tracked by `Workspace`, addressed by `NodeId`, and shared with other features. The editor is a view/controller over that document, not its persistence authority. `Workspace::save` and `Workspace::reload` own disk I/O and version/conflict handling. Agent edits, workspace search/replace, reload, LSP workspace edits, and other views can change the same open document. Dirty state and the toolbar's path/version/Save/Reload UI belong to the shared panel/workspace integration.

The CodeMirror implementation sends edit **splices** to Rust; Rust applies them to the document mirror and `Workspace::Document`. Externally-originated text is pushed back into the view. The native textarea implementation currently sends the whole string through `oninput`; that is a known limitation for large documents, not the target contract.

### Current backend boundary

`packages/code-view/src/backend/mod.rs` defines `CodeEditorBackend`, `BackendEvent`, `Splice` and `WikiMark`. The panel mounts a backend in the host element and retains the handle until unmount. A replacement can reshape this low-level boundary to suit a declarative Dioxus component, but it must preserve the behavior below; the shell, document ownership, LSP orchestration, and source semantics should not move into the editor engine.

| Direction | Current operation | Required meaning |
|---|---|---|
| Rust → editor | mount with element id, initial text, language hint, wrap setting | Show the current document; unknown/no language means plain text. Mount failure is visible and recoverable, not an endless loading state. |
| Editor → Rust | `Ready`, `Failed` | Report readiness and surface errors so panel actions do not run against an absent view. |
| Editor → Rust | `Spliced { changes, length }` | Report document edits in order; each range addresses the pre-change document and `length` checks the resulting view. Rust reconciles and can request a full resync on mismatch. |
| Rust → editor | `set_text`, `focus`, `undo`, `redo`, `set_cursor`, `set_wrap` | Apply reload/agent/LSP text, focus the view, history commands, reveal/navigation, and live settings. |
| Rust → editor | `set_diagnostics`, `hover_result`, `completion_result` | Render current LSP diagnostics; answer asynchronous hover/completion requests correlated by request id. |
| Editor → Rust | `Hover`, `Completion`, `Definition`, `Rename`, `CodeActions`, `References` | Send the current selection/caret as a zero-based LSP position/range and request work from the shared Rust LSP session. |
| Rust ↔ editor | `Cursor`, `set_presence` | Publish this document's active caret to `Workspace`; render other participants' line/initials in the gutter. |
| Rust → editor / editor → Rust | `set_wiki_links`, `WikiQuery`, `WikiLink` | Render Rust-computed resolved/unresolved `[[wiki-links]]`; request candidates and open a link on primary-modifier click in Markdown source. |
| Rust → editor | `run(EditorAction)` | Execute the same editor command exposed by shell menus/palette/keybindings. |

The current offset contract uses UTF-16 code units for edit splices, and zero-based line plus UTF-16 column for LSP positions. UTF-8 byte offsets are used by Rust text/source APIs, so conversions must be explicit and safe around non-BMP characters. The current native editor's caret conversion increments the column by Unicode scalar value while CodeMirror reports UTF-16 columns; this is a real consistency gap to close, not a behavior to preserve.

### Host behaviors the editor participates in

- One document can be shown in multiple windows/views. The canonical document remains in `Workspace`; a remount starts from its current text.
- Save paths are toolbar, Ctrl/Cmd+S, and shell command. Reload/revert and LSP/agent/workspace edits update the view from Rust. A view-originated edit must make the shared document dirty; a Rust-originated update must not be mistaken for a second independent user edit.
- The Code extension and Markdown Source mode use the same code panel. The source mode needs Markdown language support and wiki-link interaction; Rich mode is separate.
- Search/trace/go-to-definition can ask the workspace to reveal a node and line/column. Cross-file definition opens the target and places the caret; same-file definition moves the caret and scrolls it into view.
- CodeMirror's backend is mounted from the host element's `onmounted` because an effect can race the native desktop DOM. A replacement must have reliable mount/readiness and teardown across webview and browser targets.

## CodeMirror 6 survey

CodeMirror 6 is a modular editor framework, not one monolithic editor with every capability enabled. Its state/view core is extended through packages and extension values. The official extension catalogue and reference manual separate core editing/view facilities from optional language, search, completion, lint, folding, gutter, tooltip, decoration, and theme packages. The capabilities below distinguish what the framework provides from what Moonkale configures.

### General CodeMirror capability areas

- **Editing core:** document and selection state, transactions and change sets, keymaps/commands, input handling, focus, update hooks, configurable extensions, and view rendering that limits DOM work to the visible viewport plus a margin.
- **Text interaction:** single and multiple selections (when enabled), mouse/keyboard selection, rectangular selection, drag/drop cursor, bracket matching and closing, special-character display, indentation helpers, and custom input handlers.
- **Language integration:** language packages can contribute parsing/syntax highlighting, language data, indentation, folding, completion sources, and comments. Availability and behavior depend on each language package; the framework does not supply every grammar or language server.
- **Navigation and feedback:** line numbers and custom gutters, active-line highlighting, folding gutters, lint marks/panels, hover/autocomplete tooltips, and decorations (marks, inline/block widgets, replacements, and line styling).
- **Commands and assistance:** history/undo, search and replace (including query configuration such as case sensitivity and regular expressions), autocomplete UI and keymaps, lint display and navigation, folding, configurable themes, and extension-specific commands.

This list is a survey of CodeMirror's documented surface, not a requirement to recreate every optional extension. A replacement must meet Moonkale's currently configured behavior and the explicit app requirements below. The official references are linked under [[#References]].

### Features enabled by Moonkale today

| Area | Current Moonkale behavior |
|---|---|
| Editing and history | Local editing; undo/redo from keyboard and shell Edit commands; UTF-16 change splices to Rust; external `setText` uses a minimal changed range so cursor/scroll can survive an agent edit or reload. |
| Presentation/navigation | Line numbers, active line and active-line gutter, Moonkale token-based light/dark theme, primary modifier conventions (Ctrl on most systems, Cmd on Apple), soft wrap controlled by `editor.wrap`, toolbar button and Alt+Z setting action. |
| Parsing/editing helpers | Syntax grammar; fold gutter/fold and unfold all; bracket matching; indent-on-input; toggle comment for languages with grammar. Unknown grammars fall back to plain text. |
| Search | In-document find and replace panel; Mod-F opens find, Mod-H opens find/replace, next/previous search bindings and selection-match highlighting. Replacement edits flow through the same document-change path. Workspace-wide search is a separate shell feature. |
| LSP | Rust-owned shared session per language/folder. Diagnostics appear in the editor gutter; hover asks Rust and displays returned text; completions come from LSP; F12 goes to definition; F2 starts a rename prompt; Mod-. requests code actions for the selection; Shift-F12 requests references. Returned workspace edits update open documents or version-check writes to closed files. |
| Markdown source | Resolved/unresolved wiki-link styling, async `[[` completion from Rust, and Ctrl/Cmd-click to follow a target. Wiki spans are recomputed from the document/index and mapped into editor edits. |
| Presence/context | Throttled caret line/column updates reach `Workspace`; other members' initials appear beside their active line. The workspace can read the caret's word for other features. |
| Lifecycle | Async hover/completion responses are identified by request ids and time out; the panel observes `Ready`/mount failure; unmount destroys the view and closes the LSP document. |

### Language coverage actually configured

The Rust `Node::language_hint` selects the view grammar through `packages/js/codemirror/src/languages.ts`; it does not start an LSP. LSP availability is separately configured by the host/server and can be absent.

| Language hints with a CodeMirror grammar | Notes |
|---|---|
| Rust; JavaScript/JSX; TypeScript/TSX; C/C++; Go; JSON; Markdown; SQLite SQL; PostgreSQL SQL; YAML; Python; CSS; HTML | Lezer language packages. |
| Typst | `codemirror-lang-typst` Lezer grammar. |
| Julia; TOML/Pixi; Cypher; shell/Bash; Lua | Legacy stream modes. |
| Plain text fallback | Used for no hint, unknown hint, or grammar setup failure. |

GraphQL has a package dependency but is not selected by the current language map. Lean, Nix, HelixQL and TypeQL have no CodeMirror grammar today. The existing Rust `dioxus-code-editor` integration offers a broader tree-sitter/Arborium grammar set (including Lean, Nix and Typst); the replacement should not regress Moonkale's documented [[Core Languages]] set. Grammar coverage should be data-driven and independent of the LSP map.

## Replacement requirements

Priority levels: **MUST** is required for the CodeMirror replacement to be considered equivalent; **SHOULD** is expected unless a measured/platform constraint is documented; **LATER** is an explicit future capability, not a blocker for parity.

### Runtime and component boundary

- **CED-R01 — MUST: Rust editor engine.** Editing behavior and syntax processing must use a Rust editor engine/component. Do not bundle or call CodeMirror, another JavaScript editor engine, or use JavaScript eval as the editor's command/caret API. The component must work in the existing Dioxus desktop, web and Android builds where the current view is supported.
- **CED-R02 — MUST: retain Moonkale ownership boundaries.** `Workspace` remains authoritative for text, dirty state, persistence, source versioning, and LSP orchestration. The editor owns the interactive view state (selection/caret, scroll, folds, local history) and reports edits/requests through a narrow Rust API. Do not fork document state or introduce editor-specific disk access.
- **CED-R03 — MUST: shared code view.** Keep one behaviorally consistent code component for the generic Code extension and Markdown Source mode. Markdown Rich mode remains distinct. Keep editor choice/extension contribution behavior coherent while the existing opt-in native extension is resolved.
- **CED-R04 — MUST: lifecycle and failure reporting.** Mount only when the host view exists, report ready/failure, clean up listeners/tasks on unmount, and avoid stale callbacks changing another document. Failure to load an optional grammar should degrade to plain text; failure to mount should show an actionable error instead of an indefinite spinner.

### Editing, document sync and performance

- **CED-R05 — MUST: responsive incremental edits.** Send localized changes to Rust, not a full document snapshot for each keystroke. Changes must be applicable against the pre-edit text in a defined order, include enough information to detect/resync a mismatch, and avoid O(document-size) serialization per keystroke. Preserve the current UTF-16 bridge contract or explicitly migrate every caller/converter together.
- **CED-R06 — MUST: Unicode-safe coordinates.** Define and consistently convert byte offsets, scalar/grapheme movement, UTF-16 edit offsets, and zero-based UTF-16 LSP columns. No invalid slicing or caret drift for emoji, combining marks, CRLF, or end-of-line positions. Add cases for these in the eventual implementation verification.
- **CED-R07 — MUST: external text synchronization.** Reflect `Workspace` changes from reload/revert, agents, workspace replace, rename/code actions and other views. Keep the editor and shared document convergent, do not create feedback loops or mark an already-saved external update dirty a second time, and preserve caret/selection/viewport when an edit outside the selection permits it.
- **CED-R08 — MUST: editing history and focus.** Provide working undo/redo both in-editor and through `Command::Undo` / `Command::Redo`; focus and caret restoration must work after menu actions, switching panels, reload, and reveal navigation. External replacement semantics must not corrupt the user's history.
- **CED-R09 — MUST: usable large-file view.** Render large documents without constructing a DOM/text widget for every character/line outside the viewport, and keep typing/edit propagation responsive. Preserve at least the existing spec-018 3 MB document scenario (recorded Rust dirty-state update was about 87 ms); benchmark the replacement on the same setup and investigate material regressions rather than assuming parity.

### Editing behavior and presentation

- **CED-R10 — MUST: baseline code editing.** Support insertion/deletion, selection, copy/paste, keyboard and pointer caret movement, IME/composition input, multiline text, platform primary modifier conventions, focus, and plain text without a language grammar. Accessibility label/name must include the document title and the editor must remain keyboard-operable.
- **CED-R11 — MUST: Moonkale presentation.** Provide line numbers, syntax theme driven by Moonkale theme tokens (including live light/dark change), active line/gutter feedback, cursor, selection, and horizontal scrolling or soft-wrap according to the user's live `editor.wrap` setting. Retain the toolbar's path/language/version, dirty marker, Save/Reload, and wrap control semantics.
- **CED-R12 — MUST: configured language editing support.** Match all languages currently highlighted by CodeMirror and provide safe plain-text fallback for unknown language ids. Support syntax highlighting, grammar-aware indent-on-input, matching brackets, folding (including fold/unfold all), and comment toggle where the selected grammar supports the operation. Prefer coverage of all [[Core Languages]], including languages the current CodeMirror map lacks. Keep grammar choice driven by Rust's language hint, separate from LSP discovery.
- **CED-R13 — MUST: find/replace.** In-document find and replace must work from the shell `EditorAction::{Find,Replace}`, keyboard shortcuts (Mod-F and Mod-H), palette/menu and the editor itself. Search replacement must be a normal document edit so `Workspace`, dirty state, indexing and LSP observe the result. Workspace-wide search stays in the shell.
- **CED-R14 — SHOULD: CodeMirror interaction parity.** Preserve the configured rectangular selection behavior, completion navigation/dismissal, bracket matching, fold navigation, selection-match highlighting, and expected keyboard commands. Moonkale does not currently enable every optional CodeMirror feature (for example arbitrary extension plugins or its lint panel); do not expand scope to the entire CodeMirror ecosystem without a separate requirement.

### LSP integration

- **CED-R15 — MUST: diagnostics and hover.** Display Rust-delivered diagnostics at the correct range/severity with a visible gutter/mark and readable message. Update/clear markers when LSP publishes a replacement set. Hover at the pointer requests a position, displays the asynchronous response, ignores expired/stale ids and remains responsive during edits.
- **CED-R16 — MUST: completion.** Request LSP completions at the caret as the user types and on explicit completion. Display label, kind, detail and apply/insert text; accept/dismiss with keyboard and mouse; use the item-provided insertion text. Correlate async results so old responses cannot replace newer suggestions. A server-unavailable document must remain editable and not hang waiting for a result.
- **CED-R17 — MUST: navigation and refactoring requests.** F12 requests definition; same-file result moves and scrolls the caret, cross-file result opens/reveals the destination. F2 starts the existing rename flow. Mod-. requests code actions for the selection and renders/resolves them through the existing panel flow. Shift-F12 requests references and selecting a reference reveals it. Apply server workspace edits through the existing safe Rust path.
- **CED-R18 — MUST: command surface.** Implement all `EditorAction` variants: Find, Replace, Rename, CodeActions, Definition, References, ToggleComment, FoldAll, UnfoldAll. The shell command/palette/menu route and keyboard route must invoke the same behavior. Current defaults include F12, F2, Mod-., Shift-F12 and Mod-/ (when grammar support exists); primary modifier is Cmd on Apple and Ctrl elsewhere.

### Markdown links, presence and workspace context

- **CED-R19 — MUST: Markdown source wiki links.** Show resolved/unresolved `[[target]]` marks from Rust-computed spans, keep marks mapped through local edits and refresh after source/index changes, request candidate completions after `[[`, insert a well-formed link without duplicating a typed closing `]]`, and follow a target on primary-modifier click. Do not impose wiki-link behavior on non-Markdown documents.
- **CED-R20 — MUST: cursor and presence.** Report the caret's correct line/column to `Workspace` at a throttled rate and expose the word at the caret through the established workspace API. Display other members' initials on their active line. A local editor without an LSP must still provide cursor/presence behavior.
- **CED-R21 — MUST: reveal, settings and platform behavior.** Honor workspace reveal requests (line/column and scroll into view), live wrap/theme settings, Save/Reload/close and open-with behavior, and work consistently on desktop, web and Android. Do not depend on OS-only window or input APIs.

## Acceptance evidence for a later implementation

The feature suite should exercise the current CodeMirror-backed behavior through the Rust component, without requiring CodeMirror-specific DOM selectors. Existing end-to-end scenarios provide a behavior checklist: syntax highlighting and plain-text fallback (`highlight.mjs`), LSP diagnostics/hover/definition (`lsp.mjs`), completion/rename/references/code actions (`lsp2.mjs`), wiki links (`wiki.mjs`), presence (`presence.mjs`), shell Find command (`shell.mjs`), plus spec [[018]]'s 3 MB incremental-edit case. Add targeted Unicode and external-edit convergence scenarios per CED-R06/CED-R07. These are proposed acceptance criteria; no verification was run while writing this requirements note.

## Open design decisions (requirements do not settle these)

1. Which Rust editor engine best supplies a real caret/selection model, incremental edits, viewport rendering, and decorations across desktop/web/mobile? The existing textarea-over-highlight component alone is not enough for feature parity.
2. Should the new engine replace the CodeMirror backend inside `moonkale-code-view`, or should the current opt-in Rust extension be promoted and the shared panel refactored around it? Avoid maintaining two implementations without an explicit reason.
3. Which crate owns grammar sources and language-specific comment/fold/indent metadata? Avoid two independent language registries in Rust and the host.
4. What are acceptable binary size, build-time and 3 MB latency budgets on each target? Establish baseline measurements before selecting an engine.
5. Whether later features are required: semantic tokens/inlay hints, multiple carets, diagnostic quick-fix widgets, AST view and call-graph actions are described in design/planning notes but are not all implemented in the current CodeMirror surface.

## References

### Moonkale implementation and design

- `packages/code-view/src/backend/mod.rs`, `packages/code-view/src/backend/codemirror.rs`, `packages/code-view/src/panel.rs`, `packages/code-view/src/lsp.rs`
- `packages/js/codemirror/src/index.ts`, `packages/js/codemirror/src/languages.ts`, `packages/js/codemirror/PROTOCOL.md`, `packages/js/codemirror/package.json`
- `packages/editors/code/editor-code.md`, `packages/editors/code-native/editor-code-native.md`, `packages/editors/code-native/src/panel.rs`
- [[Code Editor]], [[Code Editor Implementations]], [[JS Interop Boundary]], [[LSP and Terminal]], specifications [[009]], [[010]], [[012]], [[014]], [[016]], [[018]]

### CodeMirror 6 primary documentation

- [System Guide](https://codemirror.net/docs/guide/) — state/view architecture, extension model and transactions.
- [List of Core Extensions](https://codemirror.net/docs/extensions/) — capability catalogue, including search, completion, folding, history, lint, gutters, tooltips, keymaps, language and styling.
- [Reference Manual](https://codemirror.net/docs/ref/) — APIs for state, view, search, autocomplete and lint.
- [Basic Editor Example](https://codemirror.net/examples/basic/) — features added explicitly by a typical editor setup.
- [Autocompletion Example](https://codemirror.net/examples/autocompletion/) — async/custom completion sources and result UI.
- [Lint Example](https://codemirror.net/examples/lint/) — diagnostics, gutter and actions.
- [Decorations Example](https://codemirror.net/examples/decoration/) and [Gutters Example](https://codemirror.net/examples/gutter/) — styled text, widgets, ranges and gutter markers.
