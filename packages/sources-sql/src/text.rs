//! Raw SQL execution with streaming (`fetch` cursors), parameter binding,
//! statement classification (read vs. write, for `llm::policy`), and result
//! → `QueryResult::Rows` with typed columns.
