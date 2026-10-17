# Moonkale patches

This directory vendors the published [`pulldown-cmark` 0.13.4](https://crates.io/crates/pulldown-cmark/0.13.4) (MIT, `LICENSE` included). The root `Cargo.toml` substitutes it for the crates.io copy through `[patch.crates-io]`. It is excluded from the workspace, so Moonkale's fmt and clippy gates don't apply to upstream code.

## Local patch

- `src/firstpass.rs`, `scan_next_line_or_lazy_continuation`: a whitespace-only line ends the paragraph. Without it, a list item holding only a link reference definition, followed by a whitespace-only line (a tab past the item's indent, a vertical tab or a form feed), gets an empty tight paragraph, and `OffsetIter::next` panics at `parse.rs:2199` (`Option::unwrap` on `None`). This is upstream `main`'s commit edee73b7, backported unchanged, as described in [pulldown-cmark #1129](https://github.com/pulldown-cmark/pulldown-cmark/issues/1129). Moonkale found it by fuzzing (P-212, WP-47).

**When 0.13.5 or later is released with the fix, delete this directory and the `[patch.crates-io]` entry.** Regression coverage: `packages/code-view/src/markdown_guard.rs` (the fuzz inputs, plus the printable-only inputs from #1129).
