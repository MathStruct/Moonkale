---
title: "Crash safety — containment and startup recovery"
description: "Extension task and event guards, bounded wasm retries, the web panic overlay and safe mode; checked against WP-57 on 2026-10-28."
tags: [architecture, reliability, extensions]
---
**Checked 2026-10-28 (WP-57).** Extension failures must be contained, visible
and recoverable. Recovery restores the **saved** session. Recovery snapshots of
unsaved buffers (WP-56, [[ADR-0026 Memory and the store]]) remain separate work;
a hard crash can still lose edits since the last save.

## Today, by kind

| Kind | Containment | Recovery |
|---|---|---|
| Sandboxed wasm modules, native/server | Each invocation already used a fresh wasmtime store/instance, with fuel and memory budgets. Traps now consume a session retry budget | Initial failed invocation plus at most three explicit fresh-instance retries; then disabled. Rescanning the same id preserves the budget. Commands are never replayed automatically |
| Sandboxed wasm modules, browser | Each invocation already used a fresh Worker/instance. A worker failure or 30-second timeout now consumes the same retry budget; concurrent invocation of the same module is refused | Next explicit invocation refetches the module after failure; after four failed invocations it remains disabled until page reload. Workers, timers and object URLs are released |
| Compiled-in Rust, native: panel rendering | Dioxus error boundary per panel, built 2026-10-14 | Local error detail and Reload panel; other panels remain usable |
| Compiled-in Rust, native: callbacks, tasks and resources | `ext-api::crash` guards synchronous entry points, event callbacks, future creation and every future poll. The first panic disables its owning extension for this session, cancels subsequent guarded polls, and removes its panels/contributions | One shell notice identifies the extension; restart the app to enable it again. Other extensions remain usable |
| Compiled-in Rust, web client | The current wasm client uses aborting panics: an actual panic still stops the client | A separate DOM overlay offers Reload, Copy details (local, fixed diagnostic only) and safe-mode reload. No panic payload is passed to the overlay, logging hook or server |
| External processes (LSP, terminals, Julia, agents) | Process failure stays outside the client | LSP restarts with unsaved documents; terminal/run failures remain their existing notices |

The shell wraps extension trait entry points and provides an `ExtensionGuard`
context beneath each panel and extension settings subtree. Built-in callbacks,
spawned services, effects and resources use the helpers in `ext-api::crash`.
Inputs are heap-pinned before the helpers construct async wrappers (P-234).
This keeps wrapper size independent of large extension futures; pinning inside
an async body is too late and previously crashed Chromium's renderer on
structural Preview. A 64 KiB future regression checks size, polls, cancellation
and ownership restoration. The crash suite supports Firefox and Chromium.

Compiled-in extensions supplied by other repositories must also use those helpers
for their callbacks and tasks. Raw Dioxus spawning, custom executors, foreign
threads, aborts, stack overflow and memory exhaustion cannot be contained by a
Rust unwind guard. There is no unloading or rollback of compiled Rust code.

## Startup and safe mode

`moonkale --safe` on desktop and `?safe` in the web URL disable **all extension
contributions** and start with the default layout, without overwriting saved
layout, open-document lists or extension settings. The shell frame and menus
remain available. Entering safe mode from the startup notice also resets the
current layout. Closing and restarting normally restores the user's choices.

A pending-startup marker counts an incomplete prior launch once, saturating at
255. After two consecutive incomplete startups, the next launch offers safe mode
in one notice. Thirty seconds after the frame mounts, a healthy launch clears
the streak; a web panic prevents this checkpoint. Native caught extension
failures leave the startup marker pending. A forced early close also counts as
an incomplete startup: this is a recovery heuristic, not a diagnosis.

Desktop stores only this marker in `state.sqlite`, table `health`, key `startup`
([[ADR-0014 One store for internal state]]). Web uses
`localStorage["moonkale.startup-health"]`; unavailable/corrupt storage falls back
to a fresh counter and explicit safe mode still works. No settings are deleted.
The native marker is per app process startup; the browser marker is per origin.
Concurrent browser tabs can affect that heuristic, but never saved preferences.

## Steps

1. ✅ Per-panel render boundaries and Reload (2026-10-14).
2. 📝 Unsaved edit recovery snapshots (WP-56).
3. ✅ Native extension task/event guards, with session disablement and one notice.
4. ✅ Web panic hook and independent crash overlay. Unwinding web builds remain future work.
5. ✅ Startup crash counting, safe-mode offer, `--safe` and `?safe`. The first native guarded panic disables its extension immediately; repeated failures need no additional dialogs.
6. ✅ Fresh wasm instances (already built), now with bounded explicit retries. No automatic command replay or compiled service hot reload.
7. ✅ Installed third-party modules default to wasm isolation and require explicit enablement/grants; compiling external Rust into a distribution remains an explicit build choice ([[Security]], [[Extension System]]).

Tests: `shell/tests/extension_crash.rs`, `ext-api::crash` counter/store tests,
`ext-host/tests/budgets.rs`, and `crash-safety.mjs`. The browser test extension
requires both the `crash-test` feature and debug assertions; failure injection
is absent from release builds. See [[How to Write Tests]] and the browser README.

The crash-test fixture additionally requires `?crash-test` (native: `MOONKALE_CRASH_TEST`) so enabling the test feature does not change ordinary startup panels.
