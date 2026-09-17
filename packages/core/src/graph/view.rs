//! Views: a *query result over the graph*, not the graph itself.
//!
//! Editors do not hold `Vec<Node>`. They hold a `GraphView`, which is the
//! materialised result of a `Query` (see `source::query`) plus a subscription
//! to changes. A view knows:
//!
//! - which nodes/edges are in it (a subgraph),
//! - the query that produced it (so it can be refreshed, saved, shared),
//! - a `Version` watermark so incremental updates can be applied.
//!
//! This is what the graph editor renders, what the table editor pages through
//! and what an LLM tool call returns. Keeping "the visible subgraph" a first
//! class object makes "display of subgraphs" a matter of running a query, and
//! makes the *same* subgraph openable in a 2D view, a 3D view and a table.
