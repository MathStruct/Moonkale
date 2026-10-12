//! Undo/redo history model and tree management.

use super::{CommandError, Selection, SelectionSetSnapshot};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use std::time::Duration;
use web_time::Instant;

const DEFAULT_UNDO_COALESCING_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Debug, Clone)]
pub(super) struct TextEdit {
    pub(super) start_before: usize,
    pub(super) start_after: usize,
    pub(super) deleted_text: String,
    pub(super) inserted_text: String,
}

impl TextEdit {
    pub(super) fn deleted_len(&self) -> usize {
        self.deleted_text.chars().count()
    }

    pub(super) fn inserted_len(&self) -> usize {
        self.inserted_text.chars().count()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UndoEditKind {
    Insert,
    Explicit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CoalescingEdit {
    start_before: usize,
    start_after: usize,
    deleted_len: usize,
    inserted_len: usize,
}

#[derive(Debug, Clone)]
struct UndoCoalescingState {
    group_id: usize,
    last_at: Instant,
    edit_kind: UndoEditKind,
    after_selection: SelectionSetSnapshot,
    edits: Vec<CoalescingEdit>,
}

impl UndoCoalescingState {
    fn from_step(
        group_id: usize,
        edit_kind: UndoEditKind,
        step: &UndoStep,
        last_at: Instant,
    ) -> Option<Self> {
        Some(Self {
            group_id,
            last_at,
            edit_kind,
            after_selection: step.after_selection.clone(),
            edits: coalescing_edits_for_kind(edit_kind, step)?,
        })
    }

    fn extend_with(
        &self,
        edit_kind: UndoEditKind,
        step: &UndoStep,
        now: Instant,
        timeout: Duration,
    ) -> Option<Vec<CoalescingEdit>> {
        if self.edit_kind != edit_kind {
            return None;
        }
        if edit_kind == UndoEditKind::Insert
            && now.saturating_duration_since(self.last_at) >= timeout
        {
            return None;
        }
        if self.after_selection != step.before_selection {
            return None;
        }

        let next_edits = coalescing_edits_for_kind(edit_kind, step)?;
        if self.edits.len() != next_edits.len() {
            return None;
        }

        for (previous, next) in self.edits.iter().zip(&next_edits) {
            match edit_kind {
                UndoEditKind::Insert => {
                    let previous_end = previous.start_after.saturating_add(previous.inserted_len);
                    if previous_end != next.start_before {
                        return None;
                    }
                }
                UndoEditKind::Explicit => {
                    if previous.start_after != next.start_before
                        || previous.inserted_len != next.deleted_len
                    {
                        return None;
                    }
                }
            }
        }

        Some(next_edits)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum UndoCoalescingMode {
    None,
    Insert,
    Explicit,
}

fn coalescing_edits_for_kind(
    edit_kind: UndoEditKind,
    step: &UndoStep,
) -> Option<Vec<CoalescingEdit>> {
    if step.edits.is_empty() {
        return None;
    }

    let mut edits = Vec::with_capacity(step.edits.len());
    for edit in &step.edits {
        let deleted_len = edit.deleted_len();
        let inserted_len = edit.inserted_len();
        match edit_kind {
            UndoEditKind::Insert => {
                if deleted_len != 0 || inserted_len == 0 || edit.inserted_text.contains('\n') {
                    return None;
                }
            }
            UndoEditKind::Explicit => {
                if deleted_len == 0 && inserted_len == 0 {
                    return None;
                }
            }
        }

        edits.push(CoalescingEdit {
            start_before: edit.start_before,
            start_after: edit.start_after,
            deleted_len,
            inserted_len,
        });
    }

    edits.sort_by_key(|edit| (edit.start_before, edit.start_after));
    Some(edits)
}

#[derive(Debug, Clone)]
pub(super) struct UndoStep {
    pub(super) group_id: usize,
    pub(super) edits: Vec<TextEdit>,
    pub(super) before_selection: SelectionSetSnapshot,
    pub(super) after_selection: SelectionSetSnapshot,
}

#[derive(Debug)]
pub(super) struct UndoRedoManager {
    /// All nodes in the undo tree (stable indices).
    ///
    /// - Node `0` is the root ("base" state) and has no step.
    /// - Other nodes contain one [`UndoStep`] representing an edit from `parent → node`.
    nodes: Vec<UndoNode>,
    /// Current node id in the undo tree.
    current: UndoNodeId,
    /// Count of nodes that currently hold a step (excludes root and pruned tombstones).
    step_count: usize,
    max_undo: usize,
    /// Clean point tracking (node id in the undo tree).
    clean_node: Option<UndoNodeId>,
    next_group_id: usize,
    open_group: Option<UndoCoalescingState>,
    coalescing_timeout: Duration,
}

type UndoNodeId = usize;

#[derive(Debug)]
struct UndoNode {
    parent: Option<UndoNodeId>,
    children: Vec<UndoNodeId>,
    /// The selected child branch to use for `Redo` when multiple exist.
    preferred_child: Option<UndoNodeId>,
    /// The step that produced this node (none for root and pruned tombstones).
    step: Option<UndoStep>,
}

impl UndoRedoManager {
    pub(super) fn new(max_undo: usize) -> Self {
        let max_undo = max_undo.max(1);
        Self {
            nodes: vec![UndoNode {
                parent: None,
                children: Vec::new(),
                preferred_child: None,
                step: None,
            }],
            current: 0,
            step_count: 0,
            max_undo,
            clean_node: Some(0),
            next_group_id: 0,
            open_group: None,
            coalescing_timeout: DEFAULT_UNDO_COALESCING_TIMEOUT,
        }
    }

    fn invalid_history_error(context: &str, node: UndoNodeId) -> CommandError {
        CommandError::Other(format!("Invalid undo history: {context} (node={node})"))
    }

    fn live_current_node(&self) -> bool {
        self.current == 0
            || self
                .nodes
                .get(self.current)
                .is_some_and(|node| node.step.is_some())
    }

    pub(super) fn can_undo(&self) -> bool {
        self.current != 0
            && self
                .nodes
                .get(self.current)
                .is_some_and(|node| node.step.is_some())
    }

    pub(super) fn can_redo(&self) -> bool {
        self.selected_child(self.current).is_some()
    }

    pub(super) fn undo_depth(&self) -> usize {
        let mut depth = 0usize;
        let mut node = self.current;
        let mut remaining = self.nodes.len();
        while node != 0 && remaining > 0 {
            let Some(current) = self.nodes.get(node) else {
                break;
            };
            let has_step = current.step.is_some();
            if !has_step {
                break;
            }
            depth = depth.saturating_add(1);
            let Some(parent) = current.parent else {
                break;
            };
            node = parent;
            remaining -= 1;
        }
        depth
    }

    pub(super) fn redo_depth(&self) -> usize {
        let mut depth = 0usize;
        let mut node = self.current;
        let mut remaining = self.nodes.len();
        while remaining > 0 {
            let Some(child) = self.selected_child(node) else {
                break;
            };
            depth = depth.saturating_add(1);
            node = child;
            remaining -= 1;
        }
        depth
    }

    pub(super) fn current_group_id(&self) -> Option<usize> {
        self.open_group.as_ref().map(|group| group.group_id)
    }

    pub(super) fn coalescing_timeout(&self) -> Duration {
        self.coalescing_timeout
    }

    pub(super) fn set_coalescing_timeout(&mut self, timeout: Duration) {
        self.coalescing_timeout = timeout;
    }

    pub(super) fn is_clean(&self) -> bool {
        self.clean_node == Some(self.current)
    }

    pub(super) fn mark_clean(&mut self) {
        self.clean_node = Some(self.current);
        self.end_group();
    }

    pub(super) fn end_group(&mut self) {
        self.open_group = None;
    }

    pub(super) fn push_step(&mut self, step: UndoStep, coalescible_insert: bool) -> usize {
        let mode = if coalescible_insert {
            UndoCoalescingMode::Insert
        } else {
            UndoCoalescingMode::None
        };
        self.push_step_with_mode(step, mode)
    }

    pub(super) fn push_explicit_coalescing_step(&mut self, step: UndoStep) -> usize {
        self.push_step_with_mode(step, UndoCoalescingMode::Explicit)
    }

    fn push_step_with_mode(&mut self, mut step: UndoStep, mode: UndoCoalescingMode) -> usize {
        let now = Instant::now();
        let edit_kind = match mode {
            UndoCoalescingMode::None => None,
            UndoCoalescingMode::Insert => Some(UndoEditKind::Insert),
            UndoCoalescingMode::Explicit => Some(UndoEditKind::Explicit),
        };
        let next_edits = edit_kind.and_then(|kind| coalescing_edits_for_kind(kind, &step));
        let reuse_open_group = next_edits.is_some()
            && self.clean_node != Some(self.current)
            && edit_kind.is_some_and(|kind| {
                self.open_group
                    .as_ref()
                    .and_then(|group| group.extend_with(kind, &step, now, self.coalescing_timeout))
                    .is_some()
            });

        if reuse_open_group {
            if let Some(group) = &self.open_group {
                step.group_id = group.group_id;
            }
        } else {
            step.group_id = self.next_group_id;
            self.next_group_id = self.next_group_id.wrapping_add(1);
        }

        if let Some(kind) = edit_kind.filter(|_| next_edits.is_some()) {
            self.open_group = UndoCoalescingState::from_step(step.group_id, kind, &step, now);
        } else {
            self.open_group = None;
        }

        let group_id = step.group_id;
        let parent = if self.live_current_node() {
            self.current
        } else {
            debug_assert!(
                self.live_current_node(),
                "undo history current node is stale"
            );
            self.current = 0;
            self.open_group = None;
            0
        };
        let new_id = self.nodes.len();
        self.nodes.push(UndoNode {
            parent: Some(parent),
            children: Vec::new(),
            preferred_child: None,
            step: Some(step),
        });

        if let Some(parent_node) = self.nodes.get_mut(parent) {
            parent_node.children.push(new_id);
            parent_node.preferred_child = Some(new_id);
        } else {
            debug_assert!(
                parent < self.nodes.len(),
                "undo history parent node is invalid"
            );
            self.current = new_id;
            return group_id;
        }
        self.current = new_id;
        self.step_count = self.step_count.saturating_add(1);

        self.prune_if_needed();
        group_id
    }

    pub(super) fn pop_undo_group(&mut self) -> Result<Option<Vec<UndoStep>>, CommandError> {
        let mut node = self.current;
        if node == 0 {
            return Ok(None);
        }
        let group_id = self
            .nodes
            .get(node)
            .ok_or_else(|| Self::invalid_history_error("current node is out of bounds", node))?
            .step
            .as_ref()
            .ok_or_else(|| Self::invalid_history_error("current node is stale", node))?
            .group_id;

        let mut steps: Vec<UndoStep> = Vec::new();
        while node != 0 {
            let (step, parent) = {
                let current = self.nodes.get(node).ok_or_else(|| {
                    Self::invalid_history_error("undo path node is out of bounds", node)
                })?;
                let step = current
                    .step
                    .as_ref()
                    .ok_or_else(|| Self::invalid_history_error("undo path node is stale", node))?;
                if step.group_id != group_id {
                    break;
                }
                let parent = current.parent.ok_or_else(|| {
                    Self::invalid_history_error("undo path node has no parent", node)
                })?;
                (step.clone(), parent)
            };

            if self.nodes.get(parent).is_none() {
                return Err(Self::invalid_history_error(
                    "undo path parent is out of bounds",
                    parent,
                ));
            }

            steps.push(step);

            // Remember which branch we came from so redo follows it.
            if let Some(parent_node) = self.nodes.get_mut(parent) {
                parent_node.preferred_child = Some(node);
            }

            node = parent;
        }

        if steps.is_empty() {
            return Ok(None);
        }

        self.current = node;
        Ok(Some(steps))
    }

    pub(super) fn pop_redo_group(&mut self) -> Result<Option<Vec<UndoStep>>, CommandError> {
        let Some(first) = self.selected_child_checked(self.current)? else {
            return Ok(None);
        };
        let group_id = self
            .nodes
            .get(first)
            .and_then(|node| node.step.as_ref())
            .ok_or_else(|| Self::invalid_history_error("redo child node is stale", first))?
            .group_id;

        let mut node = self.current;
        let mut steps: Vec<UndoStep> = Vec::new();
        while let Some(child) = self.selected_child_checked(node)? {
            let step = {
                let child_node = self.nodes.get(child).ok_or_else(|| {
                    Self::invalid_history_error("redo child node is out of bounds", child)
                })?;
                let step = child_node.step.as_ref().ok_or_else(|| {
                    Self::invalid_history_error("redo child node is stale", child)
                })?;
                if step.group_id != group_id {
                    break;
                }
                step.clone()
            };

            // Ensure this branch remains the selected redo path.
            let parent = self.nodes.get_mut(node).ok_or_else(|| {
                Self::invalid_history_error("redo parent node is out of bounds", node)
            })?;
            parent.preferred_child = Some(child);

            steps.push(step);
            node = child;
        }

        if steps.is_empty() {
            return Ok(None);
        }

        self.current = node;
        Ok(Some(steps))
    }

    pub(super) fn redo_branch_count(&self) -> usize {
        self.nodes
            .get(self.current)
            .map_or(0, |node| node.children.len())
    }

    pub(super) fn selected_redo_branch_index(&self) -> Option<usize> {
        let node = self.nodes.get(self.current)?;
        let selected = self.selected_child(self.current)?;
        node.children.iter().position(|c| *c == selected)
    }

    pub(super) fn select_redo_branch(&mut self, index: usize) -> Result<(), CommandError> {
        let children = self
            .nodes
            .get(self.current)
            .ok_or_else(|| {
                Self::invalid_history_error("current redo node is out of bounds", self.current)
            })?
            .children
            .clone();
        if index >= children.len() {
            return Err(CommandError::Other(format!(
                "Invalid redo branch index {} (count={})",
                index,
                children.len()
            )));
        }
        let child = children[index];
        self.ensure_live_redo_child(child)?;
        self.nodes
            .get_mut(self.current)
            .ok_or_else(|| {
                Self::invalid_history_error("current redo node is out of bounds", self.current)
            })?
            .preferred_child = Some(child);
        Ok(())
    }

    pub(super) fn selected_child(&self, node: UndoNodeId) -> Option<UndoNodeId> {
        self.selected_child_checked(node).ok().flatten()
    }

    fn selected_child_checked(&self, node: UndoNodeId) -> Result<Option<UndoNodeId>, CommandError> {
        let n = self.nodes.get(node).ok_or_else(|| {
            Self::invalid_history_error("redo parent node is out of bounds", node)
        })?;
        if n.children.is_empty() {
            return Ok(None);
        }
        let selected = if let Some(pref) = n.preferred_child
            && n.children.contains(&pref)
        {
            pref
        } else {
            match n.children.last().copied() {
                Some(child) => child,
                None => return Ok(None),
            }
        };
        self.ensure_live_redo_child(selected)?;
        Ok(Some(selected))
    }

    fn ensure_live_redo_child(&self, child: UndoNodeId) -> Result<(), CommandError> {
        let child_node = self.nodes.get(child).ok_or_else(|| {
            Self::invalid_history_error("redo child node is out of bounds", child)
        })?;
        if child_node.step.is_none() {
            return Err(Self::invalid_history_error(
                "redo child node is stale",
                child,
            ));
        }
        Ok(())
    }

    pub(super) fn prune_if_needed(&mut self) {
        while self.step_count > self.max_undo {
            if let Some(leaf) = self.find_prunable_leaf() {
                self.remove_leaf_node(leaf);
                continue;
            }

            if self.prune_root_child() {
                continue;
            }

            // No safe pruning candidate found; give up.
            break;
        }
    }

    pub(super) fn find_prunable_leaf(&self) -> Option<UndoNodeId> {
        (1..self.nodes.len())
            .filter(|id| *id != self.current)
            .filter(|id| self.nodes.get(*id).is_some_and(|node| node.step.is_some()))
            .filter(|id| {
                self.nodes
                    .get(*id)
                    .is_some_and(|node| node.children.is_empty())
            })
            .min()
    }

    pub(super) fn remove_leaf_node(&mut self, id: UndoNodeId) {
        let parent = match self.nodes.get(id).and_then(|node| node.parent) {
            Some(parent) => parent,
            None => {
                debug_assert!(self.nodes.get(id).and_then(|node| node.parent).is_some());
                return;
            }
        };
        let Some(parent_node) = self.nodes.get_mut(parent) else {
            debug_assert!(
                parent < self.nodes.len(),
                "undo history leaf parent is invalid"
            );
            return;
        };
        parent_node.children.retain(|c| *c != id);
        if parent_node.preferred_child == Some(id) {
            parent_node.preferred_child = parent_node.children.last().copied();
        }

        if self.clean_node == Some(id) {
            self.clean_node = None;
        }

        let Some(node) = self.nodes.get_mut(id) else {
            debug_assert!(id < self.nodes.len(), "undo history leaf node is invalid");
            return;
        };
        node.parent = None;
        node.children.clear();
        node.preferred_child = None;
        node.step = None;

        self.step_count = self.step_count.saturating_sub(1);
    }

    pub(super) fn prune_root_child(&mut self) -> bool {
        let Some(root) = self.nodes.first() else {
            debug_assert!(!self.nodes.is_empty(), "undo history is missing root node");
            return false;
        };
        let children = root.children.clone();
        if children.len() != 1 {
            return false;
        }
        let doomed = children[0];
        if doomed == self.current {
            return false;
        }
        if self
            .nodes
            .get(doomed)
            .is_none_or(|node| node.step.is_none())
        {
            return false;
        }

        // Re-parent all of `doomed`'s children directly under root (root now represents the state
        // *after* `doomed`).
        let adopted = match self.nodes.get_mut(doomed) {
            Some(doomed_node) => std::mem::take(&mut doomed_node.children),
            None => {
                debug_assert!(
                    doomed < self.nodes.len(),
                    "undo history pruned node is invalid"
                );
                return false;
            }
        };
        for child in &adopted {
            if let Some(child_node) = self.nodes.get_mut(*child) {
                child_node.parent = Some(0);
            } else {
                debug_assert!(
                    *child < self.nodes.len(),
                    "undo history child node is invalid"
                );
                return false;
            }
        }

        let Some(root) = self.nodes.get_mut(0) else {
            debug_assert!(!self.nodes.is_empty(), "undo history is missing root node");
            return false;
        };
        root.children = adopted;
        root.preferred_child = root.children.last().copied();

        if self.clean_node == Some(doomed) {
            self.clean_node = Some(0);
        }

        let Some(doomed_node) = self.nodes.get_mut(doomed) else {
            debug_assert!(
                doomed < self.nodes.len(),
                "undo history pruned node is invalid"
            );
            return false;
        };
        doomed_node.parent = None;
        doomed_node.children.clear();
        doomed_node.preferred_child = None;
        doomed_node.step = None;

        self.step_count = self.step_count.saturating_sub(1);
        true
    }
}

/// A persistable snapshot of the undo/redo history for a single document.
///
/// This is intended for "hot exit" workflows: persist the editor text plus this snapshot, then
/// restore both on startup so undo/redo keeps working across restarts.
///
/// Notes:
/// - This snapshot does **not** include the current document text; callers are responsible for
///   persisting/restoring the text separately.
/// - Restoring a snapshot into a different document text will produce undefined undo behavior.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct UndoHistorySnapshot {
    /// Snapshot format version for forward compatibility.
    pub format_version: u32,
    /// Undo steps (oldest → newest).
    pub undo_stack: Vec<UndoHistoryStep>,
    /// Redo steps (oldest → newest).
    pub redo_stack: Vec<UndoHistoryStep>,
    /// Configured maximum undo history depth.
    pub max_undo: usize,
    /// Clean point tracking in the linearized history (index into `undo_stack + redo_stack`).
    pub clean_index: Option<usize>,
    /// The next group id to allocate.
    pub next_group_id: usize,
    /// Currently open coalescing group id, if any.
    pub open_group_id: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
/// A single undo/redo step in a persisted history snapshot.
pub struct UndoHistoryStep {
    /// Undo group id. Grouped undo pops all adjacent steps with the same id.
    pub group_id: usize,
    /// Text edits captured by this step.
    pub edits: Vec<UndoHistoryTextEdit>,
    /// Selection set snapshot before applying this step.
    pub before_selection: UndoHistorySelectionSet,
    /// Selection set snapshot after applying this step.
    pub after_selection: UndoHistorySelectionSet,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
/// A single persisted text edit in an undo/redo step.
pub struct UndoHistoryTextEdit {
    /// Start offset in the document *before* applying the edit.
    pub start_before: usize,
    /// Start offset in the document *after* applying the edit.
    pub start_after: usize,
    /// Deleted text (from the pre-edit document).
    pub deleted_text: String,
    /// Inserted text (from the post-edit document).
    pub inserted_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
/// Persisted selection set state captured alongside undo/redo steps.
pub struct UndoHistorySelectionSet {
    /// All selections (primary + secondary); may include empty selections (carets).
    pub selections: Vec<Selection>,
    /// Index of the primary selection in `selections`.
    pub primary_index: usize,
}

/// Errors produced when restoring undo history snapshots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UndoHistoryRestoreError {
    /// The snapshot uses a newer/unknown format version.
    UnsupportedFormatVersion(u32),
    /// The snapshot contains an invalid clean point index.
    InvalidCleanIndex {
        /// The invalid clean index value found in the snapshot.
        clean_index: usize,
        /// The maximum valid clean index (`undo_stack.len() + redo_stack.len()`).
        max_index: usize,
    },
}

impl std::fmt::Display for UndoHistoryRestoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedFormatVersion(v) => {
                write!(f, "Unsupported undo history snapshot format_version={}", v)
            }
            Self::InvalidCleanIndex {
                clean_index,
                max_index,
            } => write!(
                f,
                "Invalid undo history snapshot clean_index={} (max={})",
                clean_index, max_index
            ),
        }
    }
}

impl std::error::Error for UndoHistoryRestoreError {}

const UNDO_HISTORY_SNAPSHOT_FORMAT_VERSION: u32 = 1;

impl UndoRedoManager {
    pub(super) fn snapshot(&self) -> UndoHistorySnapshot {
        let undo_nodes = self.undo_path_nodes_oldest_first();
        let redo_nodes_nearest_first = self.redo_path_nodes_nearest_first();

        let clean_index = self.clean_node.and_then(|clean| {
            if clean == 0 {
                return Some(0);
            }
            if let Some(pos) = undo_nodes.iter().position(|id| *id == clean) {
                return Some(pos + 1);
            }
            if let Some(pos) = redo_nodes_nearest_first.iter().position(|id| *id == clean) {
                return Some(undo_nodes.len() + pos + 1);
            }
            None
        });

        UndoHistorySnapshot {
            format_version: UNDO_HISTORY_SNAPSHOT_FORMAT_VERSION,
            undo_stack: self
                .undo_path_steps_oldest_first()
                .iter()
                .map(|step| UndoHistoryStep {
                    group_id: step.group_id,
                    edits: step
                        .edits
                        .iter()
                        .map(|edit| UndoHistoryTextEdit {
                            start_before: edit.start_before,
                            start_after: edit.start_after,
                            deleted_text: edit.deleted_text.clone(),
                            inserted_text: edit.inserted_text.clone(),
                        })
                        .collect(),
                    before_selection: UndoHistorySelectionSet {
                        selections: step.before_selection.selections.clone(),
                        primary_index: step.before_selection.primary_index,
                    },
                    after_selection: UndoHistorySelectionSet {
                        selections: step.after_selection.selections.clone(),
                        primary_index: step.after_selection.primary_index,
                    },
                })
                .collect(),
            redo_stack: self
                .redo_path_steps_newest_first()
                .iter()
                .map(|step| UndoHistoryStep {
                    group_id: step.group_id,
                    edits: step
                        .edits
                        .iter()
                        .map(|edit| UndoHistoryTextEdit {
                            start_before: edit.start_before,
                            start_after: edit.start_after,
                            deleted_text: edit.deleted_text.clone(),
                            inserted_text: edit.inserted_text.clone(),
                        })
                        .collect(),
                    before_selection: UndoHistorySelectionSet {
                        selections: step.before_selection.selections.clone(),
                        primary_index: step.before_selection.primary_index,
                    },
                    after_selection: UndoHistorySelectionSet {
                        selections: step.after_selection.selections.clone(),
                        primary_index: step.after_selection.primary_index,
                    },
                })
                .collect(),
            max_undo: self.max_undo,
            clean_index,
            next_group_id: self.next_group_id,
            open_group_id: self.current_group_id(),
        }
    }

    pub(super) fn restore_from_snapshot(
        &mut self,
        snapshot: UndoHistorySnapshot,
    ) -> Result<(), UndoHistoryRestoreError> {
        if snapshot.format_version != UNDO_HISTORY_SNAPSHOT_FORMAT_VERSION {
            return Err(UndoHistoryRestoreError::UnsupportedFormatVersion(
                snapshot.format_version,
            ));
        }

        let undo_steps = snapshot
            .undo_stack
            .into_iter()
            .map(|step| UndoStep {
                group_id: step.group_id,
                edits: step
                    .edits
                    .into_iter()
                    .map(|edit| TextEdit {
                        start_before: edit.start_before,
                        start_after: edit.start_after,
                        deleted_text: edit.deleted_text,
                        inserted_text: edit.inserted_text,
                    })
                    .collect(),
                before_selection: SelectionSetSnapshot {
                    selections: step.before_selection.selections,
                    primary_index: step.before_selection.primary_index,
                },
                after_selection: SelectionSetSnapshot {
                    selections: step.after_selection.selections,
                    primary_index: step.after_selection.primary_index,
                },
            })
            .collect::<Vec<_>>();

        let redo_steps = snapshot
            .redo_stack
            .into_iter()
            .map(|step| UndoStep {
                group_id: step.group_id,
                edits: step
                    .edits
                    .into_iter()
                    .map(|edit| TextEdit {
                        start_before: edit.start_before,
                        start_after: edit.start_after,
                        deleted_text: edit.deleted_text,
                        inserted_text: edit.inserted_text,
                    })
                    .collect(),
                before_selection: SelectionSetSnapshot {
                    selections: step.before_selection.selections,
                    primary_index: step.before_selection.primary_index,
                },
                after_selection: SelectionSetSnapshot {
                    selections: step.after_selection.selections,
                    primary_index: step.after_selection.primary_index,
                },
            })
            .collect::<Vec<_>>();

        let total_steps = undo_steps.len() + redo_steps.len();
        let max_undo = snapshot.max_undo.max(total_steps).max(1);
        if let Some(clean_index) = snapshot.clean_index
            && clean_index > total_steps
        {
            return Err(UndoHistoryRestoreError::InvalidCleanIndex {
                clean_index,
                max_index: total_steps,
            });
        }

        // Rebuild as a linear chain (snapshots do not persist alternative branches).
        self.nodes = vec![UndoNode {
            parent: None,
            children: Vec::new(),
            preferred_child: None,
            step: None,
        }];
        self.current = 0;
        self.step_count = 0;

        let mut node = 0usize;
        let mut undo_node_ids: Vec<UndoNodeId> = Vec::new();
        for step in undo_steps {
            let new_id = self.nodes.len();
            self.nodes.push(UndoNode {
                parent: Some(node),
                children: Vec::new(),
                preferred_child: None,
                step: Some(step),
            });
            if let Some(parent_node) = self.nodes.get_mut(node) {
                parent_node.children.push(new_id);
                parent_node.preferred_child = Some(new_id);
            } else {
                debug_assert!(
                    node < self.nodes.len(),
                    "undo history restore parent is invalid"
                );
            }
            node = new_id;
            undo_node_ids.push(new_id);
            self.step_count = self.step_count.saturating_add(1);
        }

        let current_node = node;

        // `redo_steps` in the v1 snapshot format is stored in "stack order" (newest → oldest).
        // We recreate the redo path in redo order (oldest → newest) as a simple child chain.
        let mut redo_node_ids_nearest_first: Vec<UndoNodeId> = Vec::new();
        let mut redo_parent = current_node;
        for step in redo_steps.into_iter().rev() {
            let new_id = self.nodes.len();
            self.nodes.push(UndoNode {
                parent: Some(redo_parent),
                children: Vec::new(),
                preferred_child: None,
                step: Some(step),
            });
            if let Some(parent_node) = self.nodes.get_mut(redo_parent) {
                parent_node.children.push(new_id);
                parent_node.preferred_child = Some(new_id);
            } else {
                debug_assert!(
                    redo_parent < self.nodes.len(),
                    "undo history restore redo parent is invalid"
                );
            }
            redo_parent = new_id;
            redo_node_ids_nearest_first.push(new_id);
            self.step_count = self.step_count.saturating_add(1);
        }

        self.current = current_node;
        self.max_undo = max_undo;
        self.next_group_id = snapshot.next_group_id;
        self.open_group = snapshot.open_group_id.and_then(|group_id| {
            let step = self.nodes.get(self.current)?.step.as_ref()?;
            if step.group_id != group_id {
                return None;
            }
            let kind = if coalescing_edits_for_kind(UndoEditKind::Insert, step).is_some() {
                UndoEditKind::Insert
            } else {
                UndoEditKind::Explicit
            };
            UndoCoalescingState::from_step(group_id, kind, step, Instant::now())
        });

        self.clean_node = snapshot.clean_index.and_then(|idx| {
            if idx == 0 {
                return Some(0);
            }
            let undo_len = undo_node_ids.len();
            if idx <= undo_len {
                return undo_node_ids.get(idx - 1).copied();
            }
            let redo_pos = idx.saturating_sub(undo_len + 1);
            redo_node_ids_nearest_first.get(redo_pos).copied()
        });

        Ok(())
    }

    pub(super) fn undo_path_nodes_oldest_first(&self) -> Vec<UndoNodeId> {
        let mut nodes: Vec<UndoNodeId> = Vec::new();
        let mut node = self.current;
        let mut remaining = self.nodes.len();
        while node != 0 && remaining > 0 {
            let Some(current) = self.nodes.get(node) else {
                break;
            };
            if current.step.is_none() {
                break;
            }
            nodes.push(node);
            let Some(parent) = current.parent else {
                break;
            };
            node = parent;
            remaining -= 1;
        }
        nodes.reverse();
        nodes
    }

    pub(super) fn redo_path_nodes_nearest_first(&self) -> Vec<UndoNodeId> {
        let mut out: Vec<UndoNodeId> = Vec::new();
        let mut node = self.current;
        while let Some(child) = self.selected_child(node) {
            out.push(child);
            node = child;
        }
        out
    }

    pub(super) fn undo_path_steps_oldest_first(&self) -> Vec<UndoStep> {
        self.undo_path_nodes_oldest_first()
            .into_iter()
            .filter_map(|id| {
                self.nodes
                    .get(id)
                    .and_then(|node| node.step.as_ref())
                    .cloned()
            })
            .collect()
    }

    /// Redo path steps in the legacy "redo stack order" (newest → oldest), matching the v1
    /// `UndoHistorySnapshot` semantics.
    pub(super) fn redo_path_steps_newest_first(&self) -> Vec<UndoStep> {
        let mut steps: Vec<UndoStep> = self
            .redo_path_nodes_nearest_first()
            .into_iter()
            .filter_map(|id| {
                self.nodes
                    .get(id)
                    .and_then(|node| node.step.as_ref())
                    .cloned()
            })
            .collect();
        steps.reverse();
        steps
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Position, SelectionDirection};
    use super::*;

    fn selection_snapshot() -> SelectionSetSnapshot {
        SelectionSetSnapshot {
            selections: vec![Selection {
                start: Position::new(0, 0),
                end: Position::new(0, 0),
                direction: SelectionDirection::Forward,
            }],
            primary_index: 0,
        }
    }

    fn undo_step(group_id: usize) -> UndoStep {
        UndoStep {
            group_id,
            edits: vec![TextEdit {
                start_before: 0,
                start_after: 0,
                deleted_text: String::new(),
                inserted_text: "x".to_string(),
            }],
            before_selection: selection_snapshot(),
            after_selection: selection_snapshot(),
        }
    }

    fn tombstone_node(parent: Option<UndoNodeId>) -> UndoNode {
        UndoNode {
            parent,
            children: Vec::new(),
            preferred_child: None,
            step: None,
        }
    }

    #[test]
    fn pop_undo_group_reports_stale_current_node() {
        let mut manager = UndoRedoManager::new(10);
        manager.nodes.push(tombstone_node(Some(0)));
        manager.current = 1;

        let err = manager
            .pop_undo_group()
            .expect_err("stale current node should be an explicit error");
        assert!(err.to_string().contains("current node is stale"));
    }

    #[test]
    fn pop_redo_group_reports_stale_child_node() {
        let mut manager = UndoRedoManager::new(10);
        manager.nodes.push(tombstone_node(Some(0)));
        manager.nodes[0].children.push(1);
        manager.nodes[0].preferred_child = Some(1);

        let err = manager
            .pop_redo_group()
            .expect_err("stale redo child should be an explicit error");
        assert!(err.to_string().contains("redo child node is stale"));
    }

    #[test]
    fn checked_history_accessors_do_not_panic_on_invalid_current_node() {
        let mut manager = UndoRedoManager::new(10);
        manager.current = 999;

        assert!(!manager.can_undo());
        assert!(!manager.can_redo());
        assert_eq!(manager.undo_depth(), 0);
        assert_eq!(manager.redo_depth(), 0);
        assert_eq!(manager.redo_branch_count(), 0);
        assert_eq!(manager.selected_redo_branch_index(), None);
    }

    #[test]
    fn valid_undo_redo_group_paths_still_work() {
        let mut manager = UndoRedoManager::new(10);
        let group = manager.push_step(undo_step(0), false);
        assert_eq!(group, 0);

        let undo_steps = manager.pop_undo_group().unwrap().unwrap();
        assert_eq!(undo_steps.len(), 1);
        assert_eq!(manager.current, 0);

        let redo_steps = manager.pop_redo_group().unwrap().unwrap();
        assert_eq!(redo_steps.len(), 1);
        assert_eq!(manager.current, 1);
    }
}
