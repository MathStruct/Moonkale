---
title: "Crash safety — what happens when an extension fails, and what comes back"
description: "Whether Moonkale survives an extension crashing and can reload it. Per kind of extension (wasm modules, compiled-in Rust, out-of-process tools) and per platform (native desktop, the web client, the phone): what is contained today, what was added on 2026-10-14 (a boundary around every panel), and the steps to make every failure contained, visible and recoverable without losing work."
tags: [architecture, reliability, extensions]
---
**Question (Daniel, 2026-10-14):** is Moonkale crash-safe when an extension crashes, and could it reload extensions when that happens? **Partly. It depends on the kind of extension and on the platform.** Since 2026-10-14 every extension panel has its own boundary with a **Reload** button.

## Today, by kind

| Kind | What a failure does | Recovers how |
|---|---|---|
| **Wasm extensions** (third-party, sandboxed) | a trap, an exhausted fuel budget (50 M units for the manifest, 5 G per run) or the 256 MB memory limit ends **that call** with an error; each call runs in a fresh store | ✅ the next call starts clean; nothing else is affected |
| **Out-of-process tools:** language servers, terminals, the Julia process, `claude` | the process dies; Moonkale sees it closed | ✅ language servers restart with their unsaved documents (codex's recovery, `lsp-restart.mjs`); a terminal shows that it ended; a Julia run fails as a run |
| **Compiled-in Rust extensions, native desktop:** a **panic while a panel renders** | **contained since 2026-10-14:** Dioxus catches render panics, and each panel now sits in an error boundary (`shell.rs`, `guarded`). The panel shows "*X* stopped with an error", with the message and its location and a **Reload panel** button; the rest of the window keeps working. Test: `shell/tests/extension_crash.rs` | ✅ Reload re-renders the panel |
| Compiled-in Rust, native: a panic in an **event handler** or a **spawned task** | not caught by the render boundary; it may take the window down | ⚠ restart; the session comes back from the store, minus unsaved edits until ADR-0026's recovery snapshots exist |
| Compiled-in Rust, **the web client** | the client is built with `panic = "abort"` (for size), so **any** panic stops the whole page | ⚠ a page reload; the session comes back from the server's store |
| Compiled-in Rust, **the phone** | as native, but Android may also kill the app under memory pressure | ⚠ restart restores the session |
| The **server** process | a panic in a request handler ends that request (tokio tasks are isolated); a panic in shared state can poison it | ✅ mostly; ⚠ poisoned locks to review |

So third-party code (wasm) and external tools are contained. **The weak spot is Moonkale's own compiled-in extensions** outside render, and the web client as a whole.

## The rule

A failing extension must be **contained** (only its panel or call fails), **visible** (it says what failed and where, never a blank panel), and **recoverable** (Reload, or a restart that loses no work). Data safety doesn't depend on the extension: everything authored is in the state store within a second, and unsaved edits get recovery snapshots ([[ADR-0026 Memory and the store]]).

## Steps

1. ✅ **A boundary per panel** with the error, its location and Reload (2026-10-14).
2. **Recovery snapshots of unsaved edits** (ADR-0026, WP-56), so even a hard crash loses nothing.
3. **Catch panics in spawned tasks and event handlers** on native. Extensions spawn through a shell helper that wraps the future in `catch_unwind`, reports the panic to the panel's boundary, and cancels the task instead of the app.
4. **The web client:** a global error hook shows "Moonkale stopped, Reload restores your session" instead of a frozen page. Later, if size allows, build the web client with unwinding, so panels recover there too.
5. **Crash counting and safe mode:**
   - An extension that fails three times in a minute is **disabled for the session**, with a notice and an "enable again" action.
   - An extension that crashes at startup twice in a row starts disabled next time.
   - `moonkale --safe` starts with core extensions only.
6. **Reload an extension, not only its panel:** for wasm modules, re-instantiate from the file (also after an update). For compiled-in ones, Reload is a remount of their panels and a restart of their services; there is no unloading of compiled code.
7. **Third-party code defaults to isolation:** an extension not built by the Moonkale project runs as wasm or as a separate process, never compiled in without the user choosing it ([[Extension System]], [[Security]]).

Work package: WP-57 in [[Work Packages]].
