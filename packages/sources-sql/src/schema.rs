//! Schema introspection: `information_schema` / `PRAGMA table_info` /
//! `duckdb_tables()` → `Schema`. Cached per connection, invalidated on
//! `SchemaChanged`.
