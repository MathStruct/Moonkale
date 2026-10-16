//! Bounded, pure structural planning protocol. Results are proposals, never writes.
use serde::{Deserialize, Serialize};
use std::{future::Future, pin::Pin};

/// Snapshot-bound handle for a top-level Rust function.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionHandle {
    /// Exact syntax input digest, not a content identity.
    pub input: [u8; 32],
    /// Function syntax byte range in that input.
    pub start: usize,
    /// Exclusive end of the function syntax.
    pub end: usize,
    /// Original declaration spelling.
    pub name: String,
    /// One-based source line for disambiguating equal names.
    pub line: usize,
}

/// Supported operations; changing a declaration never updates references.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Operation {
    /// Inspect the current unsaved snapshot.
    List,
    /// Change only the declaration name token.
    DeclarationName {
        /// Selected function in the request snapshot.
        target: FunctionHandle,
        /// New declaration spelling, at most 256 bytes.
        name: String,
    },
    /// Move before another same-file function, or before EOF trivia.
    Move {
        /// Function to move in the request snapshot.
        target: FunctionHandle,
        /// Same-snapshot destination, or EOF trivia when absent.
        before: Option<FunctionHandle>,
    },
}

/// A pure request, carrying no file paths or source authority.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanRequest {
    /// Exact unsaved source, limited to 512 KiB.
    pub text: String,
    /// Requested transformation or inspection.
    pub operation: Operation,
}

/// Serializable UTF-8 byte replacement against the request's exact text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ByteEdit {
    /// First replaced byte.
    pub start: usize,
    /// Exclusive last replaced byte.
    pub end: usize,
    /// Expected bytes at the range.
    pub removed: String,
    /// Proposed replacement.
    pub inserted: String,
}

/// Pure planner output; consumers still validate through DocumentEditProposal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanResponse {
    /// Supported functions in this exact snapshot, at most 4096.
    pub functions: Vec<FunctionHandle>,
    /// Proposed edits, at most 4096 and 1 MiB combined payload.
    pub edits: Vec<ByteEdit>,
}

/// Recoverable failures; raw source and parser diagnostics never become UI text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanError {
    /// The platform or server lacks this experimental feature.
    Unavailable,
    /// Source ownership or write capability does not support this action.
    ReadOnly,
    /// Both bounded planning workers are occupied.
    Busy,
    /// Source, result or protocol limits exceeded.
    Limit,
    /// Input is not supported error-free Rust.
    Syntax,
    /// A target belongs to another snapshot.
    Stale,
    /// Invalid declaration name or unsafe move.
    Invalid,
}

/// Platform future; local implementations schedule parsing on a blocking worker.
pub type PlanFuture = Pin<Box<dyn Future<Output = Result<PlanResponse, PlanError>>>>;
/// Optional Workspace service installed only by experimental builds.
pub struct StructuralPlanner(pub fn(PlanRequest) -> PlanFuture);
