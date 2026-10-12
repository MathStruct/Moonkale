//! Generic document processing interfaces.
//!
//! This module defines a shared "edit" format for derived editor state, such as:
//! - syntax / semantic highlighting (style layers)
//! - folding regions
//!
//! External crates (`editor-core-*`) can produce [`ProcessingEdit`] values and apply them to an
//! [`EditorStateManager`] via
//! [`EditorStateManager::apply_processing_edits`](crate::EditorStateManager::apply_processing_edits).

use crate::EditorStateManager;
use crate::decorations::{Decoration, DecorationLayerId};
use crate::diagnostics::Diagnostic;
use crate::intervals::{FoldRegion, Interval, StyleLayerId};
use crate::symbols::DocumentOutline;

/// A change to derived editor state (highlighting, folding, etc.).
#[derive(Debug, Clone)]
pub enum ProcessingEdit {
    /// Replace an entire style layer with the given intervals (char offsets).
    ReplaceStyleLayer {
        /// The style layer being replaced.
        layer: StyleLayerId,
        /// The full set of style intervals for the layer (char offsets, half-open).
        intervals: Vec<Interval>,
    },
    /// Clear a style layer.
    ClearStyleLayer {
        /// The style layer being cleared.
        layer: StyleLayerId,
    },
    /// Replace folding regions.
    ///
    /// This affects the **derived** fold set (from external providers), leaving user-created folds intact.
    ///
    /// Producers with asynchronous document versions must discard stale responses before creating
    /// this edit; core applies it as an authoritative replacement for the current document state.
    /// If `preserve_collapsed` is true, regions that match an existing collapsed derived region,
    /// including conservative matches across small line-number drift, remain collapsed.
    ReplaceFoldingRegions {
        /// The complete set of folding regions.
        regions: Vec<FoldRegion>,
        /// Whether to preserve the collapsed/expanded state for regions that still exist.
        preserve_collapsed: bool,
    },
    /// Clear all derived folding regions (leaves user-created folds intact).
    ClearFoldingRegions,
    /// Replace the current diagnostic list (character offsets).
    ReplaceDiagnostics {
        /// Full diagnostic list for the document.
        diagnostics: Vec<Diagnostic>,
    },
    /// Clear all diagnostics.
    ClearDiagnostics,
    /// Replace a decoration layer wholesale.
    ReplaceDecorations {
        /// Decoration layer being replaced.
        layer: DecorationLayerId,
        /// Full decoration list for the layer (character offsets).
        decorations: Vec<Decoration>,
    },
    /// Clear a decoration layer.
    ClearDecorations {
        /// Decoration layer being cleared.
        layer: DecorationLayerId,
    },
    /// Replace the document outline / symbol tree wholesale.
    ReplaceDocumentSymbols {
        /// The full document outline.
        symbols: DocumentOutline,
    },
    /// Clear the current document symbol set.
    ClearDocumentSymbols,
}

/// A generic processor that produces [`ProcessingEdit`]s for an editor document.
pub trait DocumentProcessor {
    /// The error type returned by [`DocumentProcessor::process`].
    type Error;

    /// Compute derived state updates to apply to the editor.
    ///
    /// Implementations should avoid mutating `state`; instead, return edits that the caller can
    /// apply (e.g. via [`EditorStateManager::apply_processing_edits`](crate::EditorStateManager::apply_processing_edits)).
    fn process(&mut self, state: &EditorStateManager) -> Result<Vec<ProcessingEdit>, Self::Error>;
}
