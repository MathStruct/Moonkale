---
title: "wordcount — example wasm extension"
tags: [crate-notes, milestone-6]
---
The smallest useful third-party extension for `moonkale-ext-host` (Milestone 6): a `cdylib` built with plain `cargo build --target wasm32-unknown-unknown`, no component tooling.

- Manifest: id `dev.moonkale.example-wordcount`, permission `read-sources`, commands `wordcount.count` (lines/words/characters of a file: `{source, node}`) and `wordcount.top` (`{source, node, n}` most frequent words); both `llm_tool: true`.
- `src/lib.rs` implements the guest side of the ABI in ~100 lines: `alloc`, `manifest`, `run`, a `host_call` helper that packs/unpacks the `(ptr << 32) | len` return, and the two commands over `HostCall::FetchText`.
- `build.sh` builds it in release and copies `wordcount.wasm` to `$MOONKALE_CONFIG_DIR/extensions` (default `~/.config/moonkale/extensions`). Restart the app (desktop) or the server (web), enable it in Settings → Extensions and tick *read-sources*.

Verified by `packages/ext-host/tests/wordcount.rs` (builds the module itself) and the web E2E `wasm-ext.mjs` (fixture README: 3 lines, 4 words, 19 characters; the agent shows an approval card because third-party tools are `Mutating`).
