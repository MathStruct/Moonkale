---
title: moonkale-ext-history — crate notes
tags: [crate-notes, milestone-18]
---
Notes for `moonkale-ext-history`, the History panel (Milestones 8 and 9; its own crate since Milestone 18 phase 4.2, was `shell/src/history.rs`). Where the log lives: [[Internal State]] (one row per event in the folder host's store).

- `HistoryExtension`: the side panel `history` (events newest first with when / actor / summary / key, an *active file* filter through `EntityLog::for_node`, **Compact (n)** keeping the last 200 events, the log's size) and `history-view:<event>` panels in the main tile showing `text_at`, with **Restore** when the text differs from the open document. Command `history.show`.
- `assets/history.css` — the panel's styles (were in the shell's `shell.css`).
- Reads `ws.history.log`; compaction and restore are `Workspace::compact_history` / `restore_text_at` (ext-api).
