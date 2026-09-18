//! # moonkale-trace
//!
//! "Click a stack trace into a graph" (Milestone 4, P-21). [`parse`] turns
//! terminal text into [`Trace`]s — Rust panics/backtraces, cargo/rustc
//! diagnostics, Python tracebacks, JS/Node stacks, and bare `path:line`
//! mentions — and [`TraceSource`] exposes one as a `Source` the Graph panel
//! can draw: files contain frames, frames call the next frame.
//!
//! Frame nodes are `Symbol`s whose native key is `path:line[:col]`, so any
//! consumer can open the file at the spot without knowing this crate.

pub mod parse;
pub mod source;

pub use parse::{parse, Frame, Trace};
pub use source::TraceSource;
