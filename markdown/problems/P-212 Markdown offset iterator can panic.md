---
title: "P-212 — Markdown offset iterator can panic"
tags: [problem, security, testing, markdown]
status: resolved
phase: WP-47
---
**Status:** resolved 2026-10-17 (Claude, after codex's containment). The parser itself is fixed: Moonkale vendors pulldown-cmark 0.13.4 with upstream main's one-hunk fix (edee73b7, [pulldown-cmark #1129](https://github.com/pulldown-cmark/pulldown-cmark/issues/1129)), substituted through `[patch.crates-io]` in the workspace and the fuzz crate, until 0.13.5 ships. Codex's guard stays as a second layer. **Related:** [[Security]], [[How to Write Tests]], [[Work Packages]].

## Problem

CI run [39050073252, attempt 1](https://github.com/MathStruct/Moonkale/actions/runs/39050073252/attempts/1), commit d58ad792, found a pulldown-cmark 0.13.4 panic in `OffsetIter::next` at `parse.rs:2199`. The saved artifact is 128 bytes (the initial handoff described 86). Arbitrary Markdown reaches this iterator through math, block preview, list/table/callout/image previews, wiki projection, heading lookup and documentation rendering. The native Properties front-matter recognizer does not use pulldown-cmark. Agent messages use plain iteration, which succeeds on this input; wiki snippets already reject NUL, but token exclusion did not.

## Options

| | approach | pros | cons |
|---|---|---|---|
| A | Upgrade the parser | Removes the upstream defect | 0.13.4 is still the latest published release on 2026-10-17 |
| B | Catch failures at presentation boundaries | Keeps canonical source; discards partial output | Works only with unwinding; cannot recover from wasm release aborts |
| C | Reject triggering control characters before parsing, plus B | Covers this crash on abort profiles too | NUL/vertical-tab/form-feed documents retain plain source rather than formatted presentation |

## Measurements

A standalone stable harness compiling the original production `math::displays` from HEAD reproduces all four inputs. Stable Rust reproduces the original offset-iterator panic; plain iteration completes with 12 events. Greedy character deletion reaches nine bytes, Rust literal `"- [\0]:[\n\u{b}"`. Replacing the NUL label with `a` **still crashes**, as does replacing the vertical tab with form feed: `"- [a]:[\n\u{c}"`. Exhaustive single ASCII trailing-byte probes find vertical tab (11) and form feed (12); ordinary Unicode whitespace probes do not reproduce it. There is no YAML requirement.

Upstream [#1129](https://github.com/pulldown-cmark/pulldown-cmark/issues/1129) already reports the same iterator bug. Its claim that NUL is required is narrower than the variants above. [Releases](https://github.com/pulldown-cmark/pulldown-cmark/releases) offer no newer patch/minor; no dependency was changed.

## Decision

Keep the original file bytes. Presentation boundaries reject NUL, vertical tab and form feed before parser construction, including on `panic=abort` wasm. On unwind builds, catch the whole parsing operation and discard partial output. Editor decoration providers return no decorations; hover becomes unavailable; documentation and agent messages retain literal text. Diagnostics contain fixed text only; rejected controls log once per process. No global panic hook is replaced.

The original crash and three minimized variants live in the math fuzz corpus and are normal stable regression fixtures across production entry points. The standalone fuzz target compiles the same production math parser and guard. This is containment for a known crash family, not a guarantee against every future parser defect in abort builds; upstream repair is still needed.

## Follow-up (Claude, 2026-10-17)

The control-character skip did not cover the family: a 90-second local fuzz run found a tab-only input (`p-212-tab.md`), and the upstream issue already listed printable-only reproductions such as `"- [o]:u\n    \t"` (a whitespace-only line after a list item holding only a link reference definition). On the web client (panic=abort) these would still have crashed. So the root cause is now fixed in a vendored copy (`packages/code-view/vendor/pulldown-cmark`, see its `MOONKALE_PATCHES.md`; only `src/firstpass.rs` differs from the crate, and the WP-47 checksum manifest records it). `markdown_guard::tests::patched_parser_survives_known_crash_inputs` runs the parser unguarded on every known input with empty and all options; it fails at `parse.rs:2199` on the unpatched source. A comment with the extra reproductions went to the upstream issue. **Remove the vendored copy and the `[patch.crates-io]` entries when 0.13.5 or later ships with the fix**; the control-character skip can go then too.

The next 3-minute fuzz run then found a second, smaller bug: the native math parser widened a display block back to its line start by searching for `\n` only, so with bare `\r` line endings (valid CommonMark) a block's range reached back over the previous block, and the ranges overlapped. It now stops at `\r` too and never before the previous block's end (`native_math_parser::tests::bare_cr_lines_keep_ranges_ordered_and_disjoint`, input `p-212-overlap.md`). After both fixes, 3,062,852 fuzz runs in 181 s found nothing.

## Lesson

Minimization by deletion does not establish that a particular character value is necessary. Substitute ordinary labels and whitespace too. A catch boundary must surround lazy iterator consumption, and an abort-profile client needs prevention before parser construction.
