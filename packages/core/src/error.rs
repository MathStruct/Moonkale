//! The single error type shared across the model layer.
//!
//! Crates above `core` wrap this in their own error types; crates below it
//! don't exist. `Unsupported` is important: a `Source` is allowed to say "I
//! cannot do that" (a read-only DuckDB file cannot accept a write) and the UI
//! must render that gracefully rather than as a crash.

use crate::graph::Version;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq, Error, Serialize, Deserialize)]
pub enum SourceError {
    #[error("not found")]
    NotFound,
    /// Optimistic-concurrency failure: the node changed since it was read.
    #[error("version conflict: expected {expected}, found {actual}")]
    Conflict { expected: Version, actual: Version },
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("invalid: {0}")]
    Invalid(String),
    /// I/O and driver errors, stringified so the type stays serializable
    /// across the server boundary.
    #[error("io: {0}")]
    Io(String),
}

impl From<std::io::Error> for SourceError {
    fn from(e: std::io::Error) -> Self {
        if e.kind() == std::io::ErrorKind::NotFound {
            SourceError::NotFound
        } else {
            SourceError::Io(e.to_string())
        }
    }
}
