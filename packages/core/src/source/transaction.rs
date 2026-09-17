//! Writes.
//!
//! All mutations are expressed as a `Transaction`: an ordered list of
//! `Op`s (`CreateNode`, `UpdateProps`, `ReplaceContent { patch }`,
//! `CreateEdge`, `DeleteEdge`, `DeleteNode`) with an expected `Version` per
//! touched node for optimistic concurrency.
//!
//! Text edits are carried as *patches* (a rope diff / CRDT delta, TBD — see
//! ADR on collaborative editing in the vault), never as whole-file replaces,
//! so that (a) large files are cheap to save, (b) the same op stream can be
//! replayed for undo/redo and (c) a future multi-user mode has something to
//! merge.
//!
//! The `Applied` result reports new versions and any ops the source refused
//! (`Unsupported`), so the UI can partially succeed and explain why.
