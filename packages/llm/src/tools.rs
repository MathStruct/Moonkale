//! Tool surface generation.
//!
//! Turns `CommandDescriptor`s (with `ArgSchema`) into provider-neutral tool
//! definitions, plus the built-in tools: `graph.query`, `graph.fetch`,
//! `graph.apply`, `source.text_query { dialect, text }`, `index.search`,
//! `editor.open`, `workspace.list_sources`. Results are `GraphView`s or rows,
//! serialised compactly (ids + labels first, content on request) to keep
//! context windows small.
