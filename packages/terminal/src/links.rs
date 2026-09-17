//! Link detection in terminal output: paths with line:col, URLs, and language-specific error formats (rustc, Julia stack traces, Go panics) → `Frame`/`File` nodes, feeding `moonkale-index::trace`.
