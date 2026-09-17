//! `SourceRegistry` — the open sources of a workspace, by `SourceId`.
//!
//! Also the multiplexer: a `Query` without a source filter fans out to all
//! sources whose capabilities match and merges the streams. Cross-source
//! edges are stitched here (the registry knows both ends).
