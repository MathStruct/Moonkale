# Moonkale patches

This directory vendors the published [`editor-core` 0.5.0](https://github.com/windoze/editor-core) crate (upstream source: [crates.io](https://crates.io/crates/editor-core/0.5.0)). The upstream project declares `MIT OR Apache-2.0`; the corresponding `LICENSE-MIT` and `LICENSE-APACHE` texts are included here.

## Local patch

- `src/undo.rs`: use `web_time::Instant` instead of `std::time::Instant` for undo-group timing. The standard-library clock panics on Moonkale's `wasm32-unknown-unknown` runtime. `std::time::Duration` remains unchanged.

- `src/commands.rs`, `src/state.rs`: expose `discard_undo_history` so the native view can release redundant engine transactions after handing localized deltas to Workspace. Workspace alone owns user undo/redo, including across remounts. When updating the vendored crate, compare against upstream 0.5.0 and reapply this portability patch only if still needed. Keep this note and the `web-time` dependency synchronized with the source.
