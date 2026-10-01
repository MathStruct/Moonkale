---
title: "Publishing and Platforms"
tags: [extensions, guide]
---
> [!note] Checked 2026-10-01
> What exists: wasm modules (single `.wasm` files, no folder layout or `moonkale.toml`) are discovered in `~/.config/moonkale/extensions/` and `<folder>/.moonkale/extensions/`, and served to the browser runtime by `/api/ext/module/{id}`. No installer, no registry, no version check, no platform check. A folder's modules can be enabled and granted by that folder's settings, which the audit flags (#4, #8 — [[Audit 2026-09-23]]).

## Layout of an installed extension
```text
<id>/
├─ moonkale.toml
├─ <name>.wasm            # kind = wasm
├─ grammars/ queries/ icons/
└─ README.md
```

## Install locations
| platform | path | mechanism |
|---|---|---|
| desktop | `~/.config/moonkale/extensions/<id>/` (XDG) | copy / `moonkale ext install <url\|path>` |
| server | `MOONKALE_EXTENSIONS_DIR` | admin-managed |
| web | served by the app's own server | never from arbitrary URLs |
| mobile | static only in v1; wasm modules from `files/extensions/<id>/` once the WebView runtime and an installer exist — [[Android Extensions and Bundling]] | bundled at build time; later: install from URL with hash check |

## Registry
A registry (an index JSON + tarballs) is planned but not designed. Until then: git URLs and local paths. Signing: the manifest carries a hash of the package; the registry signs the index. Decision pending — logged in [[Problem Log]].

## Platform honesty
`platforms = [...]` must match what your code needs. The host refuses to activate an extension on an unlisted platform and shows why. Native `source` contributions are auto-restricted to desktop/server; the shell surfaces them to web clients via the server.

## Versioning
Semver for the extension; `api = "^x.y"` for the host. The host refuses incompatible extensions with a clear diagnostic in the Extensions panel.
