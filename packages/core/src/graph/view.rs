//! Views (design stub — Milestone 1 uses `QueryResult` directly).
//!
//! Planned: `GraphView` = a `Query` + its materialised `QueryResult` + a
//! `Version` watermark for incremental updates. It is what the graph editor
//! renders, what the table editor pages through and what an LLM tool call
//! returns; "display a subgraph" is "run a narrower query".
