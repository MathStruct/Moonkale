---
tags: [extensions, reference]
---
# Contribution Points

Each maps to `[[contributes.<name>]]` in the manifest and a type in `ext-api/src/contrib/`. Adding a *point* is an API change; adding a *contribution* is not.

| point | fields | notes |
|---|---|---|
| `panel` | `id, title, home (side\|main\|bottom\|right), icon, when, closable, singleton` | becomes a `dioxus_workbench::Panel` |
| `editor` | `id, kinds: [NodeKind], languages, mimes, priority` | "I can open these nodes"; "Open with…" lists all matches |
| `command` | `id, title, args (schema), when, llm_tool, risk, description_for_model` | one registry feeds palette, menus, keys, agents |
| `language` | `id, aliases, extensions, first_line, tree_sitter (wasm asset), queries {highlights, injections, locals}, comments, brackets, lsp {command, args, transport}` | data only |
| `source` | `family, dialect, form (connection fields), platforms` | factory registered in code |
| `renderer` | `kinds, node_style, edge_style, popup (ui::Tree template)`; static-only: `custom_pass` | declarative for wasm |
| `llm_tool` | (flag on `command`) | see [[LLM and RAG]] |
| `keybinding` | `key, command, when, mac, linux, windows, web` | web overrides for reserved keys |
| `theme` | `id, name, vars {--wb-*, --mk-*}, tokens {syntax palette}` | shared by code, markdown code blocks, graph colours |
| `flow_library` | `blocks: [BlockKind], codegen` | see [[Flow Editor]] — likely lands as a point once the editor exists |

## `when` clauses
Mini-language modelled on VS Code: `activePanel == 'editor-code' && lang == 'rust' && !readOnly`. Keys published by the shell (`activePanel`, `lang`, `readOnly`, `hasSelection`, `sourceFamily`, `platform`) and by extensions (`myext.state`).

## Activation events
`startup`, `language:<id>`, `command:<glob>`, `view:<id>`, `source:<family>`, `node-kind:<kind>`. Prefer the narrowest; `startup` needs a justification in the README.
