//! `trait CodeEditorBackend` — the swap point.
//!
//! ```ignore
//! pub trait CodeEditorBackend {
//!     fn mount(&mut self, host_el: MountPoint, initial: &Rope) -> Result<()>;
//!     fn apply(&mut self, patch: &Patch);            // Rust → view
//!     fn set_decorations(&mut self, d: &[Decoration]);
//!     fn set_selection(&mut self, s: Selection);
//!     fn events(&self) -> Stream<BackendEvent>;      // view → Rust: edits,
//!                                                    // cursor, scroll, requests
//!     fn capabilities(&self) -> BackendCaps;         // e.g. supports_inlay_hints
//! }
//! ```
//!
//! Exactly one backend is compiled in per build via features; the shell
//! doesn't know which.

pub mod codemirror;
pub mod native;
