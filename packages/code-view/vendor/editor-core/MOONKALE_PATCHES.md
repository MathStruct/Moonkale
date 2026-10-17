# Moonkale patches

This directory vendors the published [`editor-core` 0.5.0](https://github.com/windoze/editor-core) crate (upstream source: [crates.io](https://crates.io/crates/editor-core/0.5.0)). The upstream project declares `MIT OR Apache-2.0`; the corresponding `LICENSE-MIT` and `LICENSE-APACHE` texts are included here.

## Local patch

- `src/undo.rs`: use `web_time::Instant` instead of `std::time::Instant` for undo-group timing. The standard-library clock panics on Moonkale's `wasm32-unknown-unknown` runtime. `std::time::Duration` remains unchanged.

- `src/commands.rs`, `src/state.rs`: expose `discard_undo_history` so the native view can release redundant engine transactions after handing localized deltas to Workspace. Workspace alone owns user undo/redo, including across remounts. When updating the vendored crate, compare against upstream 0.5.0 and reapply this portability patch only if still needed. Keep this note and the `web-time` dependency synchronized with the source.

- `src/commands.rs`, `src/state.rs`: expose `logical_position_to_visual_row` using the cached visual-row prefix index and binary search over wrap points. The preview height index needs rows, not horizontal cell coordinates; this avoids rescanning offscreen line text. Visible rows consult that index before scanning fold regions; only hidden-row lookups need the collapsed-fold fallback. Regression coverage compares row-only and full coordinate mapping across wrapping, Unicode, indentation and folding.

- `src/commands.rs`: derived-fold replacement preserves the incrementally updated row index when collapsed line ranges do not change. Expanded fold metadata cannot affect visibility. Regression coverage refreshes expanded regions, collapses, changes collapsed bounds and expands after the index has been populated.

- `src/commands.rs`: zero-cell visual-to-logical queries use wrap-point boundaries directly, without copying or scanning the logical line. Preview continuation-row and height queries use this path. Regression coverage checks wrapped Unicode, indentation and folded row boundaries.
