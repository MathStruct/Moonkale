//! Typed values — what a database cell, a property or a row holds.
//!
//! Milestone 2 introduces the minimum for SQL rows. The full `PropertyMap`
//! with `Ref(NodeId)` and `Vector(Vec<f32>)` (embeddings) is still to come.

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
    /// Binary data, shown by size; the bytes are fetched on demand later.
    Bytes {
        len: u64,
    },
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Null => f.write_str("NULL"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Int(i) => write!(f, "{i}"),
            Value::Float(x) => write!(f, "{x}"),
            Value::Text(s) => f.write_str(s),
            Value::Bytes { len } => write!(f, "<{len} bytes>"),
        }
    }
}
