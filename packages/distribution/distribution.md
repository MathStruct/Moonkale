---
title: moonkale-distribution — crate notes
tags: [crate-notes, milestone-18]
---
Notes for `moonkale-distribution` (Milestone 18 phase 4.1). Plan: [[Milestone 18 - Library Refactor]]; the list of extensions and their tiers: [[Extension Catalogue]].

**Which extensions an app ships.** `default_extensions()` starts with the shell's own four (`moonkale_shell::builtin_extensions()`: Explorer, Search, Settings, Extensions), always adds Graph, Links, Code and Markdown, and then one entry per Cargo feature: `code-native`, `table`, `image`, `terminal`, `terminal-native`, `agent`, `flow`, `lux`, `git`, `history` (all on by default). The desktop, web and mobile apps pass this function to the shell.

- Adding an extension: a crate under `packages/editors/` or `packages/extensions/`, a feature and one `#[cfg(feature = …)]` line here — the shell is not touched.
- `cargo check -p moonkale-distribution --no-default-features` (in CI) builds the minimum workbench.
- The layering check (`tools/check-deps.py`) lets only this crate and the apps depend on extensions and drivers.
- Opt-in tiers stay a runtime matter (Settings → Extensions); a feature decides whether the code is in the binary at all.
