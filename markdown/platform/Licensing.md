---
title: "Licensing"
description: Moonkale is MIT; what you build also carries its dependencies' licences — which ones, and how to list them.
tags: [platform, licensing]
---
**Moonkale's own code is MIT** (`LICENSE` at the repository root, `license = "MIT"` in the workspace `Cargo.toml`, 2026-09-21). Anyone may use, copy, modify and redistribute it, with the copyright notice kept.

**A built Moonkale is more than Moonkale.** A binary, an APK or the web bundle combines the code here with third-party software, each piece under its own licence. They are all permissive (no GPL/LGPL-only code is in the tree), but "MIT" is not the whole story for a distributed artefact — their notices have to travel with it, and a few have conditions of their own:

| where | what | licence |
|---|---|---|
| Rust (991 crates in `Cargo.lock`) | the bulk | `MIT OR Apache-2.0` (≈430), MIT (136), Apache-2.0 (33) |
| bundled engines | DuckDB, LadybugDB/Kuzu | MIT |
| | wasmtime + cranelift | Apache-2.0 WITH LLVM-exception |
| | Typst | Apache-2.0 |
| | tree-sitter + grammars (rust, julia, python) | MIT |
| | rustls / ring | Apache-2.0 / ISC / MIT |
| | ICU4X (`icu_*`, `zerovec`, …) | Unicode-3.0 |
| | `webpki-roots` (Mozilla's CA bundle) | CDLA-Permissive-2.0 |
| | `option-ext` | MPL-2.0 (file-level copyleft; unmodified use is fine) |
| JavaScript in the app | CodeMirror, Milkdown (ProseMirror, remark), xterm.js, KaTeX | MIT |
| fonts | KaTeX fonts (`editors/markdown/assets/katex/`) | SIL OFL 1.1 |
| the site | Quartz | MIT |

What this means in practice: a redistributed binary should ship a `THIRD-PARTY` notice file (Apache-2.0 asks for the NOTICE contents, MIT/BSD for the copyright lines, OFL for the font licence). It is not generated yet — a `dx bundle` step for the release process ([[Packaging Overview]]).

How to list everything: `cargo install cargo-license && cargo license` (Rust; `cargo about` produces the notice file), and `npx license-checker` in each `packages/js/*` package (JavaScript). The summary above was taken from `cargo metadata` on 2026-09-21.
