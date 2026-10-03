//! Typed values — what a database cell, a property or a row holds.
//!
//! Milestone 2 introduced the minimum for SQL rows; Milestone 18 phase 6.2
//! ([[ADR-0015 The core model]]) put [`Properties`] on every node and edge —
//! a factor's parameters (Lenticulum), a declaration's hash (Sophia), a
//! row's columns. `Ref(NodeId)` and `Vector(Vec<f32>)` come when a source
//! needs them.

use serde::{Deserialize, Serialize};
use std::fmt;

/// A node's or edge's properties, by name, sorted (so they compare and
/// serialize the same everywhere).
pub type Properties = std::collections::BTreeMap<String, Value>;

#[derive(Clone, Debug, Serialize, Deserialize)]
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

/// Floats compare by their bits, so `Value` is `Eq` (a NaN equals itself) and
/// nodes carrying properties stay `Eq`.
impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        use Value::*;
        match (self, other) {
            (Null, Null) => true,
            (Bool(a), Bool(b)) => a == b,
            (Int(a), Int(b)) => a == b,
            (Float(a), Float(b)) => a.to_bits() == b.to_bits(),
            (Text(a), Text(b)) => a == b,
            (Bytes { len: a }, Bytes { len: b }) => a == b,
            _ => false,
        }
    }
}

impl Eq for Value {}

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
