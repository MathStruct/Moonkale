//! Structured `Query` → SQL. Deliberately conservative: paginated `SELECT`
//! with keyset pagination, FK-based neighbour expansion, `LIKE`/FTS for text
//! search, and dialect-specific vector ops (`pgvector`, `sqlite-vec`, DuckDB
//! `array_cosine_similarity`) when the capability is present.
