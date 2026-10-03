# Changelog — the library crates

`moonkale-core`, `moonkale-ext-api` and `moonkale-graph-render` are versioned together by **git tags** `lib-vN` on [MathStruct/Moonkale](https://github.com/MathStruct/Moonkale) (no crates.io release; decided 2026-10-01). Depend by tag, never by branch:

```toml
moonkale-ext-api = { git = "https://github.com/MathStruct/Moonkale", tag = "lib-v1" }
moonkale-core    = { git = "https://github.com/MathStruct/Moonkale", tag = "lib-v1" }
```

Rules:
- A new `lib-vN` is tagged when anything public in these crates changes in a way that can break a caller; the entry below lists what and how to update.
- Additions that cannot break (a new method, a new field on a `#[non_exhaustive]` contribution struct, a new `Extension` method with a default) go into the next tag without a version bump of their own.
- `WorkspaceConfig` and its groups are the **app** side (desktop, web, mobile): they gain fields when a platform service is added, and an app built against an older tag fails to compile until it fills them — by design, so no platform silently lacks a service.

## Unreleased (spec 030) — additions, nothing breaks

**moonkale-ext-api**
- `i18n`: `tr`, `lookup`, `check`, `system_language`, `LANGUAGES`, the `t!(ws, L, "id", var = x)` macro, and `FluentArgs`/`FluentValue` re-exported.
- `Settings.language` (empty = the system's) and `Workspace::lang()`; `SettingsState.system_language`.
- `workspace::untracked(f)`: inside it `lang()` peeks instead of subscribing — for code run from untracked effects (P-151).
- `Extension::themes()` (theme files as JSON; default none) and `ShellState.theme_light`.
- The status bar message starts empty; the shell shows its own translated *Ready*.

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
