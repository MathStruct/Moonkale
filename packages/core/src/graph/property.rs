//! Typed property bags.
//!
//! `PropertyMap = BTreeMap<PropertyKey, Value>` where `Value` is a small
//! JSON-like enum extended with the types databases actually have:
//! `Null, Bool, Int(i64), Float(f64), Decimal, Text, Bytes, DateTime, Duration,
//! Uuid, List(Vec<Value>), Map(..), Ref(NodeId), Vector(Vec<f32>)`.
//!
//! `Vector` is first class so that embeddings can live on nodes and be shown
//! or compared without a special path. `Ref` lets a property *point at* a node
//! without being an edge (e.g. "author" on a page) — the index layer may
//! materialise such refs as edges when useful.
//!
//! `PropertyKey` is interned (`Arc<str>` today, a real interner later) since
//! large tables repeat the same keys millions of times.
