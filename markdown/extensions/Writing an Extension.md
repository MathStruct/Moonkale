---
title: "Writing an Extension"
tags: [extensions, guide]
---
Moonkale is extension-driven: the built-in editors are extensions. This guide takes you from an empty folder to a panel, a command, a language and a data source. Reference material: [[Manifest Reference]], [[Contribution Points]], [[Host API Reference]], [[Publishing and Platforms]].

> [!warning] Two halves — read which one you need
> **Part A** is how to write an extension **today**, against the code as it is (checked 2026-10-01). **Part B** (sections 1 onwards) is the *target design* — a `moonkale.toml` manifest, a `Host` handle, `ui::Tree` panels, WIT components — kept so the implementation is held to it; **none of Part B exists yet**. The decision record for what was built instead is [[ADR-0013 JSON ABI before components]].

# Part A — writing an extension today

There are two kinds, and they can do different things:

| | static (Rust crate) | wasm module (JSON ABI v1) |
|---|---|---|
| can contribute | panels (any Dioxus `Element`), commands + keybindings, its own settings UI, flow-editor block libraries | commands, optionally offered to the agent as tools |
| sees | the whole `Workspace` (sources, documents, settings, …) | three host calls: `list_sources`, `query`, `fetch_text`, each checked against the granted permissions |
| ships | compiled into the binary; listed in `ui::default_extensions()` | a `.wasm` file in `~/.config/moonkale/extensions/` or `<folder>/.moonkale/extensions/` |
| runs on | every platform | desktop and server (wasmtime), browser (Worker); not on Android yet |
| example | `packages/editors/image` (≈ 60 lines of extension code) | `packages/extensions/wordcount` |

## A static extension
Depend on `moonkale-ext-api` and `dioxus` (workspace versions), implement the trait, and add one line to `ui::default_extensions()` (and the crate to `ui/Cargo.toml`) — that line moves to a `distribution` crate in [[Milestone 18 - Library Refactor]].

```rust
use dioxus::prelude::*;
use moonkale_ext_api::prelude::*;

pub struct Hello;

impl Extension for Hello {
    fn manifest(&self) -> Manifest {
        // core(..) = always on; optional(..) = on, can be switched off; opt_in(..) = off until enabled
        Manifest::opt_in("dev.example.hello", "Hello", "Greets you from a side panel.")
            .with_permissions(&["read-sources"])
    }

    fn panels(&self, _ws: Workspace) -> Vec<PanelContribution> {
        vec![PanelContribution {
            id: "hello".into(),
            title: "Hello".into(),
            home: PanelHome::Side,
            closable: true,
            dirty: false,
            node: None,
            activity: Some(Activity::new("puzzle", 100, "Hello")),
        }]
    }

    fn render(&self, _panel_id: &str, ws: Workspace) -> Element {
        let n = ws.sources.read().len();
        rsx! { p { "Hello — {n} source(s) open." } }
    }

    fn commands(&self, _ws: Workspace) -> Vec<CommandContribution> {
        vec![CommandContribution::new("hello.greet", "Hello: Greet").key("Ctrl+Alt+H")]
    }

    fn run_command(&self, id: &str, mut ws: Workspace) {
        if id == "hello.greet" {
            ws.set_status("Hello!");
        }
    }
}
```

What the trait offers (`packages/ext-api/src/extension.rs`): `manifest`, `panels` (called on every shell render — read signals there to contribute one panel per open document), `render`, `on_panel_closed`, `commands`/`run_command`, `flow_libraries` (block libraries for the flow editor, see `extensions/lux`), `settings` (an `Element` shown under the extension's row in the Extensions panel; write with `Workspace::update_settings_in`). Keep state in the `Workspace` or in signals the extension owns, not in the rendered element: a panel is remounted when it is docked elsewhere.

An editor is a static extension that contributes one panel per node it opens (see `editors/image/src/extension.rs`: it filters `ws.views` for the nodes it can show).

## A wasm extension
A core wasm module built with plain `cargo build --target wasm32-unknown-unknown --release` — no component tooling. It exports `alloc(len) -> ptr`, `manifest() -> packed(ptr,len)` and `run(ptr,len) -> packed`; it may import `moonkale.log(ptr,len)` and `moonkale.call(ptr,len) -> packed`. Every value is JSON:

```jsonc
// manifest()
{ "abi": 1, "id": "dev.example.count", "name": "Count", "description": "…",
  "permissions": ["read-sources"],
  "commands": [{ "id": "count.lines", "title": "Count: lines", "description": "…",
                 "input_schema": { "type": "object", "properties": { "source": {"type":"string"}, "node": {"type":"string"} } },
                 "llm_tool": true }] }
// run() receives { "command": "count.lines", "args": { … } } and returns { "ok": true, "result": "…" } or { "ok": false, "error": "…" }
// call() sends { "op": "list_sources" } | { "op": "query", "source": "…", "query": <a core Query as JSON> }
//             | { "op": "fetch_text", "source": "…", "node": "…" }
//   and gets back { "ok": true, "result": <JSON> } or { "ok": false, "error": "…" }
```

The types are in `packages/ext-host/src/abi.rs` (a Rust guest can depend on `moonkale-ext-host` with no features to share them); `packages/extensions/wordcount/src/lib.rs` is a complete example and `build.sh` installs it. The user grants the permissions in the Extensions panel; until then the host refuses the calls.

---

# Part B — the target design (not built)

## 1. What an extension is

An extension = a **manifest** (`moonkale.toml`, declarative) + **code** (Rust, optionally compiled to a WASM component). The manifest is read first and drives the UI before your code runs; code is activated lazily.

```mermaid
flowchart LR
  M[moonkale.toml] -->|registered at startup| SHELL[shell builds menus, palette, keybindings]
  SHELL -->|activation event| CODE[your crate: activate, command, panel]
  CODE -->|Host handle| APP[graph, commands, ui, storage, llm, net]
```

Two kinds:

| kind | runs as | can render | platforms | when to choose |
|---|---|---|---|---|
| `static` | Rust crate linked into the binary | full Dioxus `Element` | all | first-party editors, anything needing wgpu or raw Dioxus |
| `wasm` | WASM component loaded at runtime | declarative `ui::Tree` | desktop, server, web (Worker) | everything distributable |

Both implement the same `Extension` trait against the same `Host`.

## 2. Create the crate

```text
my-extension/
├─ Cargo.toml
├─ moonkale.toml
└─ src/lib.rs
```

`Cargo.toml`:
```toml
[package]
name = "my-extension"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]   # cdylib for wasm, rlib for static

[dependencies]
moonkale-ext-api = "0.1"
serde = { version = "1", features = ["derive"] }
```

`moonkale.toml` (see [[Manifest Reference]]):
```toml
[extension]
id      = "dev.example.hello"
name    = "Hello"
version = "0.1.0"
api     = "^0.1"
kind    = "wasm"

[activation]
on = ["command:hello.*", "view:hello.panel"]

[[contributes.command]]
id = "hello.greet"
title = "Hello: Greet"
args = { name = "string" }

[[contributes.panel]]
id = "hello.panel"
title = "Hello"
home = "side"
```

## 3. Implement `Extension`

```rust
use moonkale_ext_api::prelude::*;

#[derive(Default)]
struct Hello { greetings: u32 }

impl Extension for Hello {
    fn activate(&mut self, host: Host) -> Result<(), ExtError> {
        host.set_status("Hello activated");
        Ok(())
    }

    fn command(&mut self, id: &str, args: Value) -> Result<Value, ExtError> {
        match id {
            "hello.greet" => {
                self.greetings += 1;
                let name = args["name"].as_str().unwrap_or("world");
                Ok(json!({ "message": format!("Hello, {name}!") }))
            }
            _ => Err(ExtError::UnknownCommand),
        }
    }

    fn panel(&mut self, _id: &str, ctx: PanelCtx) -> PanelOutput {
        ui::column([
            ui::text(format!("Greeted {} times", self.greetings)),
            ui::button("Greet", Action::command("hello.greet", json!({"name": "you"}))),
        ]).into()
    }
}

moonkale_ext_api::export!(Hello);   // static: inventory registration; wasm: component exports
```

That is a complete extension. Build:
- static: add the crate to the `desktop`/`web` binary's dependencies (first-party only);
- wasm: `cargo component build --release` → `target/wasm32-wasip2/release/my_extension.wasm`.

Install (desktop): copy the folder to `~/.config/moonkale/extensions/dev.example.hello/`. See [[Publishing and Platforms]].

## 4. Talk to the graph

The `Host` gives you the same surface every editor uses ([[Graph-Native Model]]):

```rust
// find markdown pages linking to the current node
let view = host.query(Query::neighbours(ctx.node, Direction::In).kind(EdgeKind::Links)).await?;
for node in view.nodes() { host.notify(format!("{} links here", node.label)); }

// read content
let text = host.fetch(ctx.node).await?.as_text()?;

// write (requires permissions.sources = ["write"])
host.apply(Transaction::new().update_props(ctx.node, [("reviewed", Value::Bool(true))])).await?;

// react to changes
let mut events = host.subscribe(Filter::kind(NodeKind::Page));
while let Some(ev) = events.next().await { /* … */ }
```

Queries fan out across all open sources you're permitted to read. You never see connection strings, file handles or sockets.

## 5. Contribute things

Each `[[contributes.*]]` table in the manifest is a [[Contribution Points|contribution point]]:

| you want to add… | contribute | code needed? |
|---|---|---|
| a dockable panel | `panel` | `panel()` |
| a command (palette, menus, keybindings, LLM tool) | `command` | `command()` |
| an editor for a node kind | `editor` | `panel()` keyed by node |
| a language (grammar, highlight, LSP) | `language` | no — data only |
| a database / data source | `source` | `SourceFactory` impl (native) |
| node/edge visuals in the graph view | `renderer` | no (declarative) / yes (custom) |
| a keybinding, a theme | `keybinding`, `theme` | no |
| an LLM tool | `command` with `llm_tool = true` | `command()` |

Worked examples: [[Example - Hello Panel]], [[Example - Data Source]], [[Example - Language]].

## 6. Permissions

Declare what you need; users grant on first use; every `Host` call is checked.

```toml
[permissions]
sources = ["read", "write"]
network = ["https://api.example.com/*"]
process = false
llm     = true
```

A denied call returns `ExtError::Denied(Capability)` — handle it, don't unwrap. See [[Host API Reference]].

## 7. State and panels

Panel content is remounted on structural moves (docking into another group). Keep state in your extension struct or in `host.kv_*` storage, not in the panel tree. Static extensions can use Dioxus signals; the rule is the same: state lives outside the panel.

## 8. Platforms

Declare `platforms = ["desktop", "web"]` if you depend on a capability that isn't everywhere (process spawning, native drivers). Check the [[Platform Matrix]]. A `source` contribution using a native driver is desktop/server-only automatically; the shell offers it to web users through the server.

## 9. Testing

- `moonkale-ext-api` ships a `TestHost` (in-memory sources, recorded calls) so `command()` and `panel()` can be unit-tested with plain `cargo test`.
- `moonkale dev --extension ./my-extension` (planned) runs the desktop app with your extension hot-reloaded on rebuild.

## 10. Versioning

`api = "^0.1"` is checked against `moonkale-ext-api`'s version. Breaking API changes bump the major and get a migration note in [[Problem Log]]. Contribution *points* are part of the API; contribution *instances* are not.

## Checklist before publishing
- [ ] manifest validates (`moonkale ext check`)
- [ ] permissions are minimal
- [ ] commands have `title`, `args` schema and, if agent-safe, `llm_tool = true` with a `risk`
- [ ] panels keep state outside the tree
- [ ] `platforms` is honest
- [ ] README with screenshots
