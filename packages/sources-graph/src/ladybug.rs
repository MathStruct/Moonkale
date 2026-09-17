//! LadybugDB source via the official `lbug` crate (LadybugDB is the
//! continuation of Kuzu; `lbug::{Database, SystemConfig, Connection}`,
//! Cypher, sync API — wrap calls in `spawn_blocking`).
//!
//! Embedded, no server: the best first graph backend for a local-first
//! user. Build notes (from the crate docs): by default `lbug` downloads a
//! prebuilt static `liblbug`; if unavailable it compiles the C++ library
//! with cmake. `LBUG_SHARED` / `LBUG_LIBRARY_DIR` / `LBUG_INCLUDE_DIR` link
//! against a system install; `LBUG_BUILD_FROM_SOURCE` forces a source build.
//! Extensions (FTS, vector) require extra linker flags for binaries — see
//! the crate README's "Using Extensions" section before enabling them.
//!
//! See vault: `research/Database Backends.md`.
