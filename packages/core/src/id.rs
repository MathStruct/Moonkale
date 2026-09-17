//! Stable identifiers.
//!
//! Every entity in Moonkale has a globally unique, *stable* id. Stability
//! matters more than it first looks: layouts are persisted by id, extensions
//! refer to nodes by id, LLM tool results cite ids, and a graph database row
//! may be re-read a thousand times and must map to the same node each time.
//!
//! Planned newtypes (all `Copy`, `Serialize`, `Deserialize`, `Display`):
//!
//! - `NodeId`, `EdgeId`  – UUID v7 for locally created entities. For entities
//!   that originate in a source (a file path, a table row, a graph vertex) the
//!   id is *derived* deterministically from `(SourceId, native key)` so that
//!   re-opening the same source yields the same ids without a lookup table.
//! - `SourceId`          – identifies one connection / one opened folder.
//! - `WorkspaceId`       – a set of sources opened together.
//! - `ExtensionId`       – reverse-DNS string (`"dev.moonkale.editor-code"`).
//! - `CommandId`         – `"<extension>.<command>"`.
//!
//! Design note: ids are opaque to editors. A graph editor must not parse a
//! `NodeId` to discover "this is a file". That information lives in the node's
//! `kind` (see `graph::NodeKind`).
