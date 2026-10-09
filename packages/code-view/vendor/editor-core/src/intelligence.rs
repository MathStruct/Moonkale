//! UI-agnostic language intelligence data models.
//!
//! This module provides a small set of **typed** schemas that hosts can use to represent common
//! “code editor intelligence” surfaces in a consistent way across UIs:
//!
//! - references result collections
//! - call hierarchy (incoming/outgoing)
//! - type hierarchy (supertypes/subtypes)
//!
//! The core idea is that integrations (LSP, Tree-sitter, bespoke engines) can populate these
//! structures and store them in [`WorkspaceIntelligence`] for later consumption by UI layers.

use crate::symbols::{SymbolKind, SymbolLocation, Utf16Range};
use std::collections::BTreeMap;

/// Opaque id for an intelligence result set stored in a workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResultSetId(u64);

impl ResultSetId {
    /// Return the underlying numeric id.
    pub fn get(self) -> u64 {
        self.0
    }
}

/// High-level kind of a stored result set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultSetKind {
    /// A references/search-like collection of locations.
    References,
    /// Call hierarchy (incoming/outgoing call edges).
    CallHierarchy,
    /// Type hierarchy (supertypes/subtypes).
    TypeHierarchy,
}

/// A generic cross-file “hierarchy item” (call hierarchy / type hierarchy).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HierarchyItem {
    /// Display name (e.g. function/type name).
    pub name: String,
    /// Optional detail string (e.g. signature).
    pub detail: Option<String>,
    /// Coarse kind tag (usually from LSP's `SymbolKind`).
    pub kind: SymbolKind,
    /// Cross-file location for navigation.
    pub location: SymbolLocation,
    /// Preferred selection range within the target (usually a tighter span than `location.range`).
    pub selection_range: Utf16Range,
    /// Optional raw integration payload, encoded as JSON text.
    pub data_json: Option<String>,
}

/// A references result collection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferencesResultSet {
    /// UI-facing title for the collection (e.g. "References: foo").
    pub title: String,
    /// Reference locations.
    pub locations: Vec<SymbolLocation>,
    /// Whether any referenced document has changed since this collection was produced.
    pub is_stale: bool,
}

impl ReferencesResultSet {
    fn mentions_uri(&self, uri: &str) -> bool {
        self.locations.iter().any(|loc| loc.uri == uri)
    }
}

/// A call hierarchy incoming edge (`from -> root`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallHierarchyIncomingCall {
    /// The caller item.
    pub from: HierarchyItem,
    /// Callsite ranges within `from` (UTF-16 ranges).
    pub from_ranges: Vec<Utf16Range>,
}

impl CallHierarchyIncomingCall {
    fn mentions_uri(&self, uri: &str) -> bool {
        self.from.location.uri == uri
    }
}

/// A call hierarchy outgoing edge (`root -> to`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallHierarchyOutgoingCall {
    /// The callee item.
    pub to: HierarchyItem,
    /// Callsite ranges within the root item (UTF-16 ranges).
    pub from_ranges: Vec<Utf16Range>,
}

impl CallHierarchyOutgoingCall {
    fn mentions_uri(&self, uri: &str) -> bool {
        self.to.location.uri == uri
    }
}

/// A call hierarchy result set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallHierarchyResultSet {
    /// UI-facing title for the collection (e.g. "Call Hierarchy: foo").
    pub title: String,
    /// Prepared root items (some servers return multiple).
    pub roots: Vec<HierarchyItem>,
    /// Incoming calls (if resolved).
    pub incoming: Vec<CallHierarchyIncomingCall>,
    /// Outgoing calls (if resolved).
    pub outgoing: Vec<CallHierarchyOutgoingCall>,
    /// Whether any referenced document has changed since this collection was produced.
    pub is_stale: bool,
}

impl CallHierarchyResultSet {
    fn mentions_uri(&self, uri: &str) -> bool {
        self.roots.iter().any(|it| it.location.uri == uri)
            || self.incoming.iter().any(|c| c.mentions_uri(uri))
            || self.outgoing.iter().any(|c| c.mentions_uri(uri))
    }
}

/// A type hierarchy result set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeHierarchyResultSet {
    /// UI-facing title for the collection (e.g. "Type Hierarchy: Foo").
    pub title: String,
    /// Prepared root items (some servers return multiple).
    pub roots: Vec<HierarchyItem>,
    /// Supertypes (if resolved).
    pub supertypes: Vec<HierarchyItem>,
    /// Subtypes (if resolved).
    pub subtypes: Vec<HierarchyItem>,
    /// Whether any referenced document has changed since this collection was produced.
    pub is_stale: bool,
}

impl TypeHierarchyResultSet {
    fn mentions_uri(&self, uri: &str) -> bool {
        self.roots.iter().any(|it| it.location.uri == uri)
            || self.supertypes.iter().any(|it| it.location.uri == uri)
            || self.subtypes.iter().any(|it| it.location.uri == uri)
    }
}

/// A stored intelligence result set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntelligenceResultSet {
    /// References/search-like results.
    References(ReferencesResultSet),
    /// Call hierarchy results.
    CallHierarchy(CallHierarchyResultSet),
    /// Type hierarchy results.
    TypeHierarchy(TypeHierarchyResultSet),
}

impl IntelligenceResultSet {
    /// Return the high-level kind of this result set.
    pub fn kind(&self) -> ResultSetKind {
        match self {
            Self::References(_) => ResultSetKind::References,
            Self::CallHierarchy(_) => ResultSetKind::CallHierarchy,
            Self::TypeHierarchy(_) => ResultSetKind::TypeHierarchy,
        }
    }

    /// Return the UI-facing title.
    pub fn title(&self) -> &str {
        match self {
            Self::References(s) => s.title.as_str(),
            Self::CallHierarchy(s) => s.title.as_str(),
            Self::TypeHierarchy(s) => s.title.as_str(),
        }
    }

    /// Return whether the result set is stale.
    pub fn is_stale(&self) -> bool {
        match self {
            Self::References(s) => s.is_stale,
            Self::CallHierarchy(s) => s.is_stale,
            Self::TypeHierarchy(s) => s.is_stale,
        }
    }

    /// Mark the result set as stale.
    pub fn mark_stale(&mut self) {
        match self {
            Self::References(s) => s.is_stale = true,
            Self::CallHierarchy(s) => s.is_stale = true,
            Self::TypeHierarchy(s) => s.is_stale = true,
        }
    }

    fn mentions_uri(&self, uri: &str) -> bool {
        match self {
            Self::References(s) => s.mentions_uri(uri),
            Self::CallHierarchy(s) => s.mentions_uri(uri),
            Self::TypeHierarchy(s) => s.mentions_uri(uri),
        }
    }
}

/// Workspace-owned storage for language intelligence result sets.
#[derive(Debug, Default)]
pub struct WorkspaceIntelligence {
    next_id: u64,
    sets: BTreeMap<ResultSetId, IntelligenceResultSet>,
}

impl WorkspaceIntelligence {
    /// Return the number of stored result sets.
    pub fn len(&self) -> usize {
        self.sets.len()
    }

    /// Returns `true` if there are no stored result sets.
    pub fn is_empty(&self) -> bool {
        self.sets.is_empty()
    }

    /// List all stored ids in deterministic order.
    pub fn ids(&self) -> Vec<ResultSetId> {
        self.sets.keys().cloned().collect()
    }

    /// Get a stored result set by id.
    pub fn get(&self, id: ResultSetId) -> Option<&IntelligenceResultSet> {
        self.sets.get(&id)
    }

    /// Get a mutable stored result set by id.
    pub fn get_mut(&mut self, id: ResultSetId) -> Option<&mut IntelligenceResultSet> {
        self.sets.get_mut(&id)
    }

    /// Remove a stored result set.
    pub fn remove(&mut self, id: ResultSetId) -> Option<IntelligenceResultSet> {
        self.sets.remove(&id)
    }

    /// Clear all stored result sets.
    pub fn clear(&mut self) {
        self.sets.clear();
    }

    /// Insert a new result set and return its generated id.
    pub fn create(&mut self, set: IntelligenceResultSet) -> ResultSetId {
        let id = ResultSetId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        self.sets.insert(id, set);
        id
    }

    /// Convenience helper: create a references result set.
    pub fn create_references(
        &mut self,
        title: impl Into<String>,
        locations: Vec<SymbolLocation>,
    ) -> ResultSetId {
        self.create(IntelligenceResultSet::References(ReferencesResultSet {
            title: title.into(),
            locations,
            is_stale: false,
        }))
    }

    /// Mark any result set that mentions `uri` as stale.
    ///
    /// Returns `true` if at least one result set changed.
    pub fn mark_stale_for_uri(&mut self, uri: &str) -> bool {
        let mut changed = false;
        for set in self.sets.values_mut() {
            if set.is_stale() {
                continue;
            }
            if set.mentions_uri(uri) {
                set.mark_stale();
                changed = true;
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbols::{Utf16Position, Utf16Range};

    #[test]
    fn mark_stale_for_uri_marks_matching_sets_only() {
        let mut intel = WorkspaceIntelligence::default();

        let a_loc = SymbolLocation {
            uri: "file:///a.rs".to_string(),
            range: Utf16Range::new(Utf16Position::new(0, 0), Utf16Position::new(0, 1)),
        };
        let b_loc = SymbolLocation {
            uri: "file:///b.rs".to_string(),
            range: Utf16Range::new(Utf16Position::new(1, 0), Utf16Position::new(1, 1)),
        };

        let a_id = intel.create_references("refs a", vec![a_loc.clone()]);
        let b_id = intel.create_references("refs b", vec![b_loc.clone()]);

        assert!(!intel.get(a_id).unwrap().is_stale());
        assert!(!intel.get(b_id).unwrap().is_stale());

        assert!(intel.mark_stale_for_uri("file:///a.rs"));
        assert!(intel.get(a_id).unwrap().is_stale());
        assert!(!intel.get(b_id).unwrap().is_stale());

        // No-op on a second mark.
        assert!(!intel.mark_stale_for_uri("file:///a.rs"));
    }
}
