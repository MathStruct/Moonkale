# Changelog — the library crates

`moonkale-core`, `moonkale-ext-api` and `moonkale-graph-render` are versioned together by **git tags** `ext-api-YYYY-MM-DD` on [MathStruct/Moonkale](https://github.com/MathStruct/Moonkale) — the date of the tagged commit; a second tag on the same day gets `-2` (no version counter: the API tag and the app's releases are independent, and `ext-api-*` matches none of the release workflow's `v*`/`moonkale-*`/`YYMMDD-proto*` patterns; decided 2026-10-12). Depend by tag, never by branch:

```toml
moonkale-ext-api = { git = "https://github.com/MathStruct/Moonkale", tag = "ext-api-2026-10-12" }
moonkale-core    = { git = "https://github.com/MathStruct/Moonkale", tag = "ext-api-2026-10-12" }
```

Rules:
- A new tag is cut when anything public in these crates changes in a way that can break a caller; the entry below lists what and how to update. (`lib-v1`, the first state's original name, stays as a tag — moonkale-julia v0.1.0 depends on it.)
- Additions that cannot break (a new method, a new field on a `#[non_exhaustive]` contribution struct, a `Default`-finished literal, a new `Extension` method with a default) ride along without demanding a release in lockstep.
- `WorkspaceConfig` and its groups are the **app** side (desktop, web, mobile): they gain fields when a platform service is added, and an app built against an older tag fails to compile until it fills them — by design, so no platform silently lacks a service.

## Unreleased — 2026-10-28 (WP-56)

- Additive workspace recovery methods use the existing platform state callbacks.
  Save, explicit reload and confirmed close remove recovery records. Native
  `installed_local_state` callbacks now run I/O on worker threads; futures still
  resolve only after completion. No configuration fields or Source APIs change.

## Unreleased — 2026-10-28 (WP-61)

- `SettingsFile` and resolved `Settings` gain `keymap` and per-button `mouse`
  maps. Older files retain the Moonkale/default mouse map; full struct literals
  need these fields or `..Default::default()`.
- `Workspace.navigation` / `NavigationState` retain up to 256 window-local
  document/camera visits. `visit_camera`, `navigate` and `load_relative_path`
  are additive. Foreground document activation records visits; guarded
  background loading does not activate or record a visit.
- `Command` adds `ToggleGraphScope`, `RunDocument` and `SplitEditor`; callers
  with exhaustive matches must handle them. `Keybinding::matches_event`,
  `display_with_layout` and `punctuation_code` support physical punctuation
  and platform labels; existing logical matcher/display methods remain.
- graph-render adds seven-coordinate `navigation_camera_state` /
  `restore_camera` snapshots, retaining the original `camera_state` API, and
  `openBeside` events on unmoved middle clicks. Middle drags always pan.
  Host event consumers must tolerate/handle the new event kind.

## Unreleased — 2026-10-28 (WP-64)

- `SettingsFile.shortcut_nudges` (optional) and resolved
  `Settings.shortcut_nudges` (default true) control once-per-command mouse hints.
  Full struct literals must include the new fields or use `..Default::default()`;
  existing persisted settings load with hints enabled.

- Additive: shell-provided `CommandBindings`, `CommandHint`, `command_hint` and
  `IconButton.command` share effective platform-formatted shortcuts across UI
  surfaces. Existing explicit `shortcut` props remain supported.
- Export `workspace::is_derived_cache` for consistent cache refresh visibility.

## Unreleased — 2026-10-28 (WP-57)

- Fix P-234: heap-pin input futures before constructing crash-guard wrappers.
  Large structural-preview tasks no longer exhaust the wasm stack. Public
  helper calls and awaited results remain the same.

- Add `Workspace::crashes` and the `crash` helpers: `ExtensionGuard`, guarded
  `event`, `spawn`, `spawn_forever`, `use_effect`, `use_future`, `use_resource`,
  startup counter and state-store tracker. The shell guards extension entry
  points and supplies ownership context. Compiled-in consumers should replace
  raw Dioxus callbacks/spawning with these helpers to contain native panics;
  `event` supports both synchronous and asynchronous callbacks; returned futures
  are polled under the same owner.
- A guarded panic disables the owning extension for the session. This is
  containment, without rollback or unloading; aborting wasm uses a page overlay.
  Safe mode refuses workspace wasm command execution without changing settings.

## Unreleased — ADR-0030 (2026-10-26)

- Additive: `Workspace::index_node` resolves a graph selection to a unique
  current index node (P-233), returning `None` for unsupported or ambiguous
  store selections. No caller migration is required.

- `SettingsFile.code_graph` and `Settings.code_graph` expose the persisted
  `auto | in-memory` graph backend preference. Add the fields to explicit
  struct literals or finish them with `Default`.
- `Command::CheckCodeCache` is a new variant; update exhaustive matches.
- `SourcesState::code_graph_status_epoch` is bumped when a code graph's cache status changes; only status displays read it.

## Unreleased — WP-21 (2026-10-24)

- Core `Source::refresh_batch(&[NodeId])` preserves an upstream watcher
  debounce window. Its default calls `refresh` sequentially, so existing
  implementations remain source compatible. The workspace and server proxy
  forward batches; the optional Sophia index writer commits one file batch.

## Unreleased — WP-11 / WP-15 (2026-10-23)

- Breaking: core `Value` gains explicit serde variants `DateTime(DateTimeCell)`,
  `Ref { node, label }` and `Vector(NumericVector)`. Update exhaustive matches.
  Existing variants retain their wire encodings. Numeric vectors widen f32 to
  f64, cap at 4096 finite samples and enforce that cap while deserializing.
  Dates hold a UTC instant and optional validated original offset.
- Core adds chrono without clock/default features for validated date arithmetic
  and parsing; no I/O or timezone database enters core.
- `Symbol.path` explicitly closed absolute `M…Z` subpaths fill with the block's
  colour token; open subpaths remain strokes. No Symbol fields change.
- `EditorFile` / `EditorSettings` gain `flow_grid` (default 16, zero off);
  literal constructors need the field or a default tail. Renderer adds
  `GraphView::set_grid`; internal `Graph` gains grid spacing.
- Canvas-layer `CardDrag` gains `unsnapped` for Alt bypass.

## Unreleased

**Breaking for Rust struct literals:** `SourceDescriptor` gains `truth: SourceTruth` (ADR-0028). Add `truth: Default::default()` to existing constructors. Old serialized descriptors still deserialize as `Files`; new values serialize as `"files"`/`"store"`. This is content ownership, not a write permission. No tag is cut by this change.

**Folder source version semantics (WP-23):** Query node versions are cheap metadata change hints, while `fetch_text`/`fetch_bytes` return exact-byte content hashes. Always pass the fetched text version to `WriteText.expected`; a listing version is not a save precondition. Workspace state/history/settings writers now fetch first and refuse failed reads. The wire type remains `Version(u64)`; no tag is cut here.

**moonkale-core**
- **Breaking for exhaustive matches (WP-06):** `Query` gains `Coarse { level: u32, region: Option<NodeId> }`. Hierarchical sources can delegate to `source::coarse::query`; other sources must return Unsupported. Shared `source::coarse::project` returns bounded hierarchy cuts, member counts and aggregated edge multiplicities, preserving distinct edge owners. Sophia uses metadata/indexed references to avoid term traversal (P-229). Existing query wire forms are unchanged; older servers reject the new variant. No tag is cut here.
- **Breaking for struct literals:** `Capabilities` gains `extra_text_query: Option<TextDialect>`, a second dialect the source also answers; add `extra_text_query: None` (or finish with `..Default::default()`). Missing in serialized descriptors, it reads as `None`. `Capabilities::text_dialects()` lists both, main first. `TextDialect` gains `SophiaQuery` (wire name `query`, Sophia SPEC-007); exhaustive matches must handle it (2026-10-21).
- Additive `graph::code` string constants define the shared L2 vocabulary; new kinds use existing `Custom` values. `NodeKind::definition()` constructs the new kind and `is_definition()` reads it and legacy `Symbol` values. No enum variants, fields, or legacy wire encodings change.

**moonkale-graph-render**
- Additive `hold_camera()` stops initial-load auto-fit before host-driven region deltas; explicit Fit/Relayout can enable it again. Ordinary delta behavior remains compatible.

**moonkale-ext-api**
- Followed sources now reload open documents without unsaved edits when the change feed names them or resets, comparing text as well as version (2026-10-21). No API change.
- `EditorAction` gains six `Native*` variants for Rust editor palette commands (reload, hover, completion, Markdown/doc preview, CodeMirror switch). Exhaustive callers must handle these; other backends may ignore them (WP-69).
- Additive `workspace::grouped_edits` composes bounded immutable document proposals, coordinates all-buffer apply/undo/redo with persistent editor adapters, retires unsafe history and reports independent per-file saves. `RevisionedDocument::apply_group` / `history_group` expose opaque group boundaries; ordinary document APIs retain their behavior. No source write occurs during apply/history.
- Save completion acknowledges the exact submitted text/version rather than a newer live buffer. Edits during an in-flight save remain dirty, and recorded history uses the submitted character count (P-206).
- Additive `structural` protocol and optional `StructuralPlanner` platform service support bounded pure inspection/declaration-name/move requests, snapshot-bound function handles and byte-edit responses. `DocumentEditBase::validate` rejects stale asynchronous results without mutation; `DocumentEditProposal::changes` exposes validated review ranges. The experimental UI/server integration stays off by default.
- Additive `Workspace::document_edit_base` and `workspace::proposals` capture bounded, single-document edits, preview them and apply through existing editor revisions/history. Typed conflicts retain proposals; application never saves. Files/write authority and live session/source identity are rechecked.
- Writes of the folder's `.moonkale/settings.json` never overlap (P-201): a save during a write asks for one more run, each run writes the current state against the version it just read, and an unchanged file isn't rewritten. `SettingsState` gains a private field for it; it was only ever built by the workspace.
- `settings::DEFAULT_CODE_EDITOR` centralizes the future backend cutover (still `codemirror`). Resolving `editor.implementation = "native"` activates the shipped Rust extension without changing stored files; explicit extension disabling and per-document choices retain priority. Unknown implementation strings resolve to the release default.
- `icons` (new module): `Icon` (moved from the shell, with toolbar icons added: save, reload, wrap, swap, layout, fit, zoom-in, zoom-out, actual-size, code, close), `IconButton` (a symbol, its description and shortcut as the tooltip, its label as hidden text), and `PanelSettings` with `PanelSettingsButton` (the gear that opens an extension's settings beside its panel; the shell provides the context).

## ext-api-2026-10-14-2

The second tag of the day: turned blocks and captions (ADR-0024 §5–6).

**Breaking:** each of these fails to compile only where a caller builds the struct literally.
- `moonkale-ext-api::flow::Block.turn` and `Block.mirror` are new fields. *Update:* a `Block` literal that lists every field adds `turn: 0, mirror: false`. Literals finished with `..Default::default()` keep compiling.
- `moonkale-ext-api::flow::BlockPorts.caption` is a new field. *Update:* a `BlockPorts` literal adds `caption: None`.
- `moonkale-graph-render::graph::Node.turn`, `.mirror` and `.caption` are new fields. *Update:* a `Node` literal adds them. Nothing outside the crate builds one today.

**moonkale-ext-api**
- `Block.turn` (quarter turns clockwise, 0–3) and `Block.mirror` (left to right in the block's own frame, before the turn). Both are omitted from the file at their defaults. `Block::rotate()` turns a block a quarter clockwise as seen; `Block::flip()` mirrors it left to right as seen, whatever its turn.
- `BlockKind.caption: Option<String>` (defaulted) and `BlockPorts.caption`: a template under the block's name. A `{name}` hole takes the name of the block at the other end of a wired port of that name, else the parameter's value or its default, else `?`.

**moonkale-graph-render**
- `InNode.turn` / `.mirror` turn and mirror a symbol's drawing inside the turned box the host sends; `InDeltaNode` carries them as options, and absent keeps the old values. A symbol with a bottom port puts its label to its right.
- `InNode.caption` / `InDeltaNode.caption`: a second, smaller line under the label.

## ext-api-2026-10-14

Block symbols (ADR-0024): a kind can draw as a schematic instead of a box, and ortho wires follow their ports' sides.

**Breaking:** each of these fails to compile only where a caller builds the struct literally.
- `moonkale-ext-api::flow::BlockPorts.symbol` is a new field. *Update:* a `BlockPorts` literal adds `symbol: None`. `BlockKind.symbol` itself is defaulted, so kinds finished with `..Default::default()` keep compiling.
- `moonkale-graph-render::graph::Node.symbol` is a new field. *Update:* a `Node` literal adds `symbol: None`. Nothing outside the crate builds one today.

**moonkale-ext-api**
- `flow::Symbol { width, height, path, anchors }`, `flow::Anchor { port, side, offset }` and `flow::Side { Left, Right, Top, Bottom }`. A symbol's `path` is SVG path data in its `0 0 width height` box, stroked in the block's colour. Its anchors place ports on the box's sides. `BlockKind.symbol: Option<Symbol>` (defaulted) and `BlockPorts.symbol` carry it.

**moonkale-graph-render**
- `InNode.symbol` / `InDeltaNode.symbol`: path data in the node's box (`0 0 2·width 2·height`). The label overlay strokes it, the body draws as a faint tint (still the hit area), and the label sits centred below. A delta without one keeps the old symbol.
- The ortho route reads its ports' sides. Ports that face each other meet half-way (the three segments as before). Ports facing the same way meet beyond the farther one, and ports facing away detour round both nodes instead of running through them.

## ext-api-2026-10-13-2

The second tag of the day (the bipartite rule, ADR-0022; the subflow scopes, ADR-0021 stage B; the editor's LSP and preview work).

**Breaking:** each of these fails to compile only where a caller builds the struct literally or matches exhaustively.
- `moonkale-ext-api::Command::RestartLanguageServer` is a new variant. *Update:* an exhaustive `match` on `Command` adds the arm.
- `moonkale-ext-api::flow::BlockPorts.role` is a new field. *Update:* a `BlockPorts` literal adds `role: None`. `BlockKind.role` itself is defaulted, so kinds finished with `..Default::default()` keep compiling.
- `moonkale-graph-render::graph::Node.sized` is a new field. *Update:* a `Node` literal adds `sized: false`. Nothing outside the crate builds one today.

- Added `media::math::{KATEX_DIR, KATEX_RENDERER}` for shared local KaTeX CSS/fonts and isolated TeX presentation. Rich and native preview share the asset folder; document/editor APIs are unchanged.

- Added `media::image` format, MIME, dimensions and size-limit helpers shared by native preview and the image editor. Existing image-editor public helper names remain reexports.

- Added `Workspace::load_node_guarded(node, current)` for background loading without activation. Existing open documents retain unsaved text; a stale fetch is discarded.
- Wiki resolution accepts same-note heading targets and creation retains an explicit `.md` suffix. Existing callers remain source-compatible.
- The code-view crate exports `ExternalLinkOpener`, registered through workspace services for native preview URL navigation.

**moonkale-ext-api**
- Native Markdown preview renders bullet/ordered/nested list items and task status as independent source-owned replacements. Caret/selection reveals the active item; task markers use the canonical edit/undo history. Complex unsupported items retain source.
- Bipartite factor graphs (ADR-0022 stage A): `flow::Role { Variable, Factor }` and `BlockKind.role: Option<Role>` (defaulted, so kinds finished with `..Default::default()` keep compiling), plus `BlockPorts.role`. **A `BlockPorts` literal must add `role: None`.** `flow::bipartite_refusal(&a, &b)` and `flow::channel_refusal(&ports, block, pin, wires)` give the rule's wording. `validate` reports a pin wire between two variables or two factors, and a factor channel carrying more than one wire, at the root and inside subflows. Kinds without a role are outside the rule.
- Subflow scopes (ADR-0021 stage B, nothing breaking): `flow::BOUNDARY` (the `boundary/<port>` pseudo-block id prefix), `Flow::scope(&path)` and `Flow::with_scope(&path, f)` (the blocks and wires a stack of subflow ids addresses — the scope an editor writes to), and `flow::inner_ports(&flow, &subflow, &libraries, &id)` (a block's ports inside a subflow: a boundary pseudo-block resolves through the declaration, a declared input is a source inside, an output a sink). `validate` now also checks inside subflows — inner wire type-checks, required boundary ports fed from inside, inner dataflow cycles, and subflow id uniqueness — with inner issues about `subflow/id`.
- Native Markdown preview covers whole documents with uncapped reveal state, changed-block reuse, sparse row heights and visible-window geometry. Canonical CRLF bytes and Workspace history remain intact; mixed-font visual navigation and incremental CRLF syntax edits are corrected.
- Editor LSP updates now negotiate incremental synchronization with UTF-16 ranges and preserve full-sync fallback. Both rename prompts ignore IME confirmation keys; CodeMirror rejects rename submissions if its prompt snapshot or server connection changed.
- Added `Command::RestartLanguageServer`, exposed as `editor.restartLanguageServer` in the palette without a default shortcut. Shared per-language/root recovery refills after five healthy minutes and supports manual restart with retained unsaved documents. Callers exhaustively matching `Command` must handle the new variant.

**moonkale-graph-render**
- `InNode.width` sets a circle's radius too, not only a rect's half-width. `Node.sized` marks a radius the host set, which the degree recompute leaves alone. **A `Node` literal must add `sized`**; nothing outside the crate builds one. A flow panel uses it to draw a factor graph's variables (ADR-0022).

## ext-api-2026-10-13

**Breaking** — build against the `Block` addition (ADR-0021 stage A):
- `moonkale-ext-api::flow::Block.subflow: Option<String>`: the subflow a block renders as one block — its id in the same document. The field is `#[serde(default)]`, so every existing flow *parses* unchanged; a caller that constructs `Block` literally does not *compile*. *Update:* add `subflow: None`, or build the fields you set and finish with `..Default::default()`.

**Additions, nothing else breaks**
- `flow::Subflow` and `Flow::subflows` (both defaulted): a named subsystem in the same document — `Subflow { id, name, blocks, wires, inputs, outputs, connections }`, the boundary declaration mirroring a `BlockKind`'s three port lists. Two blocks may reference the same subflow.
- `Flow::subflow(id)`, and `Flow::ports_of(&libraries, &block) -> Option<BlockPorts>`: a block's ports resolved from its subflow's declaration or its library kind — the one resolution site the flow panel and `validate` share. A subflow's port names must stay unique across its lists, and subflows may nest but not reference themselves; both are validation issues.
- `validate`: a wire into a subflow block type-checks against the declared boundary; a block referencing a subflow the document does not hold reports `references the subflow …, which this document does not hold`.
- `Workspace::restore_documents(folder, keys, active)` loads saved tabs in the background and restores the active tab only if foreground navigation has not superseded it. Existing open documents retain their edits.
- Document activation clears stale cursor/selection/presence context. Closing a document clears its pending reveal without resetting workspace reveal ordering. Cross-window moves refuse dirty offers and retain an origin edited after the offer; save before moving an unsaved document.
- Added `Workspace::follow_wiki_guarded` to cancel stale editor navigation after lookup/loading. `follow_wiki` now opens the resolved node in its original source instead of searching other folders for the same relative path.
- Added `RevisionedDocument::apply_in_place` for validated transactions without cloning the resulting snapshot; existing `apply` remains available.
- New public `editor` module: UTF-8 byte edit batches, UTF-16 selections, revisioned document sessions and bounded transaction undo/redo.
- `DocsState.editor_sessions`, `editor_views` and editor choice/reveal state; `Workspace::editor_session`, `editor_view_state`, `editor_view_state_with_cleanup`, `shared_state`, `set_editor_selection` and guarded reveal. Editor sessions survive remounts; close/rename disposes their resources.
- **Source compatibility:** `EditorFile` and `EditorSettings` gain `insert_spaces` and `indent_width`. Update explicit struct literals with these fields or use `..Default::default()` where supported. New public `DocsState` fields also affect explicit literals.
- **Behavior change:** `word_at` columns count UTF-16 code units rather than Unicode scalar values. Update callers that calculate columns after non-BMP characters.
  (The editor-API items above were under `Unreleased` when this section was cut; the tag contains them, so they describe it.)

## ext-api-2026-10-12

**Breaking** — build against the `BlockKind` additions:
- `moonkale-ext-api::flow::BlockKind.connections: Vec<Port>` (ADR-0020): undirected, domain-typed pins (`PortType::Named` names the domain both ends must share). The field is `#[serde(default)]`, so every existing flow and library *parses* unchanged; a library that constructs `BlockKind` literally does not *compile*. *Update:* add `connections: vec![]`, or build the fields you set and finish with `..Default::default()` (see below) — the next field then needs no release.

**Additions, nothing else breaks**
- `BlockKind: Default` — the non-breaking path above.
- `Flow::topological_skipping(&skip)`: the directed dataflow order ignoring the wires `skip` names (pin wires are equations and may loop); `Flow::topological()` keeps its meaning — the Lux codegen walks it.
- `i18n`: `tr`, `lookup`, `check`, `system_language`, `LANGUAGES`, the `t!(ws, L, "id", var = x)` macro, and `FluentArgs`/`FluentValue` re-exported.
- `Settings.language` (empty = the system's) and `Workspace::lang()`; `SettingsState.system_language`.
- `workspace::untracked(f)`: inside it `lang()` peeks instead of subscribing — for code run from untracked effects (P-151).
- `Extension::themes()` (theme files as JSON; default none) and `ShellState.theme_light`.
- The status bar message starts empty; the shell shows its own translated *Ready*.
- `moonkale-core`: `source::risk::statements(text)`: how many statements a text holds. `classify_sql` and `classify_cypher` return `Risk::Unknown` for more than one (a read-only source refuses them; the agent asks). `classify_cypher` ignores string literals and counts `LOAD`, `EXPORT`, `IMPORT`, `INSTALL`, `ATTACH`, `USE`, transactions and `CALL name = value` as writes (audit fixes, 2026-10-04).

## lib-v1 — 2026-10-03 (Milestone 18)

The first tagged state. Compared to the untagged code of Milestone 17:

**moonkale-core**
- `Node::props` and `Edge::props` (`graph::Properties`, a sorted map of `Value`): typed properties, empty by default and not serialized when empty. `Value` is `Eq` (floats compare by bits). *Update:* struct literals of `Node`/`Edge` add `props: Default::default()`.
- `NodeId::from_content(digest)`: content-addressed ids in their own namespace.
- `Source::classify(dialect, text)` and `source::risk` (the read-only gate per source); `SourceOpener` / `Openers` (which paths open as which database).
- Design settled in ADR-0015: no `GraphView`, no `subscribe` — `changes_since` is the change protocol.

**moonkale-ext-api**
- `PanelContribution`, `Activity`, `FileMark`, `CommandContribution`, `Manifest` are `#[non_exhaustive]`. *Update:* `PanelContribution::new(id, title, home)` with `.closable(…)`, `.dirty(…)`, `.node(…)`, `.activity(…)`; `FileMark::new(letter, class, title)`; `Activity::new`, `CommandContribution::new`, `Manifest::{core, optional, opt_in}` as before.
- `Workspace` state grouped by area (`ws.sources.open`, `ws.docs.active`, `ws.settings.resolved`, `ws.history.log`, …); the methods stay on the facade.
- `WorkspaceConfig { folders, processes, persistence, network, runtimes, services }` (was one flat struct). `services` + `Workspace::service::<T>()`: platform services whose types extensions define. `Processes::git` and the `git` module are gone (git's types live in `moonkale-ext-git`).
- `Extension::claims(&Node) -> Option<u8>` (which editor shows a document), `Extension::locales()` + `i18n` (strings per language), `FileMark` / `ws.contrib.file_marks`.
- `Persistence::{state, host, user_settings_in_state}` and `StateAccess`; `state_get/put`, `host_get/scan/write` on the workspace.
- `SettingsFile::without_authority`: the folder scope cannot set a provider, agent, shell, SSH host, grant, auto-approval or embeddings; `Settings::ignored_from_folder` names what it tried.
- Every public item documented (`#![warn(missing_docs)]`; CI builds the docs with `-D warnings`).

**moonkale-graph-render**
- `scene::{Scene, Event}`: the renderer as a Rust library — graph in, events out, no browser; `Event` serializes to the wasm module's JSON. `examples/headless.rs`, `README.md`.

## Unreleased — WP-63 (2026-10-28)

`SettingsFile` gains optional `density`, `chrome` and `chrome_initialized` fields;
`Settings` gains `density: Density`. Callers constructing these public structs
literally need the new fields or `..Default::default()`. Persisted settings keep
backward-compatible defaults. `icons::ToolbarActions`, `ToolbarMenu`,
`ChromeOwner` and `chrome_visible` are new shared chrome helpers.
