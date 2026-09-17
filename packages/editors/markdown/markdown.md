---
title: "editor-markdown — implementation notes"
tags: [crate-notes, milestone-2]
---
Notes for `moonkale-editor-markdown` (Milestone 2). Design: [[Markdown and Typst Editor]].

Only the index-backed part exists so far: `LinksExtension` contributes a **Links** panel (home `side`, listed after the Explorer in the default layout so it doesn't steal the active tab — P-052). `LinksPanel` re-queries on active document / `graph_epoch` / source changes: `Neighbours{depth 1, Both}` on the index, then splits `Links` edges into *Backlinks* (edges into the document) and *Outgoing* (edges out of it). Unresolved targets are phantom `Page` nodes shown in amber, not clickable; files open on click. WYSIWYG (Milkdown), the block model, link completion and Typst preview are still design notes.
