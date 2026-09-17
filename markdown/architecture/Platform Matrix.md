---
tags: [architecture, platform]
---
# Platform Matrix

The brief: *all targets are served; the apps need not be identical; single-platform features are separated.* This note is the source of truth for "what runs where" and drives the crate split in [[Project Structure]].

## Capability matrix

| Capability | Desktop (webview) | Web (browser) | Mobile (webview) | Server (`api`) |
|---|---|---|---|---|
| Workbench UI, all editors | ✅ | ✅ | ✅ (adapted layout) | — |
| Open local folder | ✅ `std::fs` + `notify` | ⚠️ OPFS / FS Access API (Chromium) / **server folder** | ⚠️ scoped dir via picker, no watch | ✅ |
| SQL/graph/KV drivers | ✅ | ❌ → `RemoteSource` | ❌ → `RemoteSource` | ✅ |
| Language servers | ✅ spawn (`lsp-local`) | ⚠️ websocket to server | ⚠️ websocket to server | ✅ spawns |
| Terminal | ✅ PTY (`terminal-pty`) | ⚠️ remote PTY on server | ⚠️ remote PTY | ✅ PTY |
| Graph view (wgpu) | ⚠️ WebGPU availability varies by webview; WebGL2 fallback | ✅ WebGPU / WebGL2 | ⚠️ WebGL2 | — |
| GPU compute layouts | ⚠️ needs WebGPU | ✅ where WebGPU | ❌ CPU layout | — |
| Static extensions | ✅ | ✅ | ✅ | ✅ |
| WASM extensions | ✅ wasmtime | ⚠️ browser runtime, Worker | ❌ v1 | ✅ wasmtime |
| Secrets | OS keychain | never client-side | Keychain/Keystore | env / vault |
| LLM providers | direct or via server | via server | via server | direct |
| Typst compile | ✅ in-process | ✅ in-process (wasm) | ✅ | ✅ |
| tree-sitter | ✅ wasm grammars | ✅ | ✅ | ✅ |

✅ native · ⚠️ works with caveats / via server · ❌ not available

## How the split is enforced

```mermaid
flowchart TB
  subgraph everywhere["compiles everywhere (incl. wasm32)"]
    core & ext-api & sources & index & lsp & terminal & llm & editors
  end
  subgraph native["native only"]
    sql[sources-sql] & gdb[sources-graph] & kv[sources-kv] & lspl[lsp-local] & pty[terminal-pty]
  end
  subgraph cfg["cfg-gated modules"]
    fs[project-fs::platform::{native,web,mobile}]
    host[ext-host::runtime::{static,wasmtime,browser}]
    surf[editor-graph::render::surface]
  end
  api --> native
  desktop --> native
  web -.never.-> native
```

- **Separate crate** when the feature is absent on some platform (drivers, PTY, LSP spawn).
- **cfg-gated module** when the feature exists everywhere with different backends (folders, extension runtime, GPU surface).
- The `web` crate must never list a native-only crate in its `Cargo.toml`; a CI check greps for it.

## Platform-specific UX
- **Desktop**: full workbench; multiple windows later (Dioxus desktop supports it).
- **Web**: same workbench; "open folder" defaults to a server-side folder; the browser's reserved shortcuts (`Ctrl+W`, `Ctrl+T`) get alternate keybindings via `KeybindingContribution.web`.
- **Mobile**: one panel at a time with a tab strip; the workbench layout collapses (`dioxus-workbench` is CSS-themable; a mobile shell wrapper is a small `ui` component). Primary use: reading/annotating the knowledge graph, light edits, chatting with the agent. Not a primary coding surface.

## Security notes for the remote paths
Remote terminal and remote LSP turn `api` into a **code-execution service**. Requirements before shipping either: authenticated sessions, per-user working-directory jail, resource limits, an allow-list of spawnable binaries, and audit logging (shared with [[LLM and RAG]]'s audit). Tracked as a high-risk item in [[Problem Ranking]].
