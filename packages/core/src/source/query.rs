//! The query model.
//!
//! Two levels, deliberately:
//!
//! 1. **Structured `Query`** — a small, source-independent IR that every
//!    source must support: "nodes of kind X under node Y", "neighbours of Z
//!    within depth 2", "page N of edges of kind K", "full-text/vector search
//!    for S". Sources translate this into `ls`, `SELECT`, `MATCH`, `SCAN`…
//!    This is what the UI and the graph view use by default so that *every*
//!    source works with *every* editor.
//!
//! 2. **`TextQuery { dialect, text }`** — raw SQL / Cypher / TypeQL / Redis
//!    commands, passed through verbatim. This is what the SQL editor sends
//!    and what LLM agents are allowed to run (after a policy check, see
//!    `moonkale-llm::policy`). Results come back as `QueryResult::Rows` and
//!    are *lifted* into nodes/edges by heuristics the source provides
//!    (primary keys become node ids, foreign keys become edges).
//!
//! `QueryResult` is always paginated / streamed. Nothing in core assumes a
//! result fits in memory.
