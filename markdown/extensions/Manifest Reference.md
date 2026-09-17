---
tags: [extensions, reference]
---
# Manifest Reference — `moonkale.toml`

```toml
[extension]
id          = "dev.example.unison"     # reverse-DNS, stable forever
name        = "Unison Language"
version     = "0.1.0"                  # semver
api         = "^0.1"                   # moonkale-ext-api requirement
kind        = "wasm"                   # "wasm" | "static"
platforms   = ["desktop", "web"]       # omit = all; see Platform Matrix
description = "…"
repository  = "https://…"
license     = "MIT"

[permissions]                          # all optional, default deny
sources = ["read"]                     # "read" | "write"
network = ["https://share.unison-lang.org/*"]
process = false                        # desktop/server only
fs      = []                           # extra paths outside the workspace
llm     = false
clipboard = false
secrets = false

[activation]
on = ["language:unison", "command:unison.*", "view:unison.codebase"]

[[contributes.language]]
id = "unison"
extensions = [".u"]
tree_sitter = "grammars/unison.wasm"
queries = { highlights = "queries/highlights.scm" }
comments = { line = "--", block = ["{-", "-}"] }
lsp = { transport = "stdio", command = "ucm", args = ["lsp"] }

[[contributes.panel]]
id = "unison.codebase"
title = "Codebase"
home = "side"
icon = "icons/codebase.svg"

[[contributes.command]]
id = "unison.pull"
title = "Unison: Pull from share"
args = { project = "string" }
llm_tool = true
risk = "mutating"

[[contributes.keybinding]]
key = "ctrl+alt+u"
command = "unison.pull"
web = "ctrl+alt+shift+u"

[[contributes.source]]
family = "custom"
dialect = "unison-codebase"
form = [{ name = "path", kind = "directory", label = "Codebase directory" }]
platforms = ["desktop"]
```

Assets (`grammars/`, `queries/`, `icons/`) are relative to the manifest and packaged with it. Unknown keys are errors (not warnings) so typos don't silently disable features.
