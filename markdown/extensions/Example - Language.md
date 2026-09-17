---
tags: [extensions, example]
---
# Example — Language

Languages are **data-only** contributions. This is the Unison entry that will live in `editors/code/src/languages/unison.rs` (as a static contribution) — a third-party extension would put the same in `moonkale.toml`:

```toml
[[contributes.language]]
id          = "unison"
aliases     = ["u"]
extensions  = [".u"]
tree_sitter = "grammars/unison.wasm"          # compiled grammar; same file on all platforms
queries     = { highlights = "queries/highlights.scm", locals = "queries/locals.scm" }
comments    = { line = "--", block = ["{-", "-}"] }
brackets    = [["(", ")"], ["[", "]"], ["{", "}"]]
lsp         = { transport = "stdio", command = "ucm", args = ["lsp"], platforms = ["desktop"] }
```

What happens:
1. [[Indexing]] loads the grammar, highlights `.u` files, extracts `Symbol` nodes via `locals.scm`.
2. The [[Code Editor]] receives highlights as decorations — CodeMirror needs no Unison mode.
3. On desktop, `lsp-local` discovers `ucm` and starts it; on web, the server does, if installed.
4. `lang == 'unison'` becomes available in `when` clauses for other contributions.

No Rust code is required. If you *do* want code (a codebase browser panel for Unison's database-backed codebase), add a `panel` and a `source` — see [[Example - Data Source]].
