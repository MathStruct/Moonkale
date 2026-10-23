---
title: "P-229 — Sophia full graph traversal can exhaust its worker stack"
tags: [problem, graph, sophia, performance]
status: partly-fixed
phase: WP-06
---
**Status:** coarse-query path fixed; upstream full traversal remains open · **Related:** [[Case Selector]], [[Graph View]], [[Problem Log]] P-224

## Problem

The first coarse-query experiment delegated to `SophiaSource::query(All)` with
an expanded two-million-node cap. On the disposable copy of the P-224 generated
100k-name store (`target/next9/gen100000.sophia`, 649 MiB), the Tokio blocking
worker exhausted its stack and aborted before returning a graph. The previous
100k cap kept most definition records outside the returned slice. Increasing
that cap reached code paths the earlier measurement did not exercise.

The pinned Sophia interface's `all` enumerates names and records and calls
`edges_at` for definition nodes. That path calls `definition_of` and recursively
walks terms to infer Calls. The abort was observed inside the worker; no native
stack trace establishes the exact overflowing frame, so that causal detail is
an inference from the implementation, not a measured backtrace.

## Options

| | approach | pros | cons |
|---|---|---|---|
| A | Raise the worker stack | easy experiment | does not establish a traversal bound |
| B | Coarse queries use namespace/name/id/indexed-reference metadata | no term decode, fits the coarse navigation purpose | does not infer Calls; full traversal still needs an upstream fix |
| C | Bound/rewrite Sophia term decoding/walks | fixes the underlying traversal | requires a separate Sophia change/tag and regression fixture |

## Measurements

Native development profile, pinned Sophia rev `4c2aaab`, disposable store copy:
- Initial full-snapshot attempt: worker stack overflow and process abort;
  `target/coarse-sophia-copy.log`.
- Metadata-only coarse path: **199,257 input nodes → 2 representatives**, zero
  projected cross-region edges; **3,682.8 ms cold**, then **1,826.3 / 1,811.6 ms**.
  `target/coarse-sophia-copy-fixed.log`.
- Existing public sample-store tests still verify namespace members, complete
  root counts and shared-definition ownership. The generated store remains a
  local reproduction; no 649 MiB binary is added to the repository.

## Decision

Use B for WP-06: read metadata on the blocking worker, preserve real namespace
Contains/Names and indexed References, assign shared definitions to one stable
name for navigation, then run the bounded shared projection. Check the store log
position before/after metadata listing and refuse a changed snapshot. Coarse
navigation does not infer Calls or load definition bodies. No stack-size change,
dependency update or upstream modification is made.

The original All/neighbour/body paths are not declared fixed. An upstream
bounded traversal plus a small hermetic stored-term regression remains open.
The cold copied-store query also exceeds two seconds; source-native cached
cuts remain an optimization task. The synthetic source-native browser acceptance
is measured separately and does not hide this limit.

## Lesson

A hierarchy summary must not require decoding every term in the source. Prefer
indexed relations and metadata; increasing a presentation cap can reach dormant
recursive paths even when the final visible graph is tiny.
