//! Typed property bags (design stub — not needed by Milestone 1).
//!
//! Planned: `PropertyMap = BTreeMap<PropertyKey, Value>` where `Value` is a
//! small JSON-like enum extended with the types databases actually have
//! (`Decimal, DateTime, Uuid, Ref(NodeId), Vector(Vec<f32>)`). `Vector` is
//! first class so embeddings can live on nodes; `Ref` lets a property point
//! at a node without being an edge.
