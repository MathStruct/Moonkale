//! HelixDB source. Two modes behind one `Source`:
//!
//! - **embedded**: `helix-db` crate with the `embedded` feature —
//!   `Client::open(HelixDbSource::Disk { root, database }).await`; also
//!   `Memory` and S3-compatible object storage. Async. Same request/response
//!   contract as the server.
//! - **server**: `helix start <name>` → `POST http://localhost:6969/v2/query`
//!   with an operation-tree body (HelixQL). Plain HTTP, so this mode also
//!   works from the *browser* build — a rare browser-direct source.
//!
//! Graph + vector in one engine makes Helix a strong candidate for the RAG
//! index backend too (`moonkale-index::embed`).
