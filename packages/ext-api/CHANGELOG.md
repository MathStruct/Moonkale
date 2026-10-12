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
