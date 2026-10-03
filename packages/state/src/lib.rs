//! # moonkale-state
//!
//! The store for Moonkale's **own** state — settings, layouts, saved agents
//! and connections, agent sessions, the entity log, projects, index caches —
//! as opposed to the user's data, which stays in sources. Secrets never go
//! here. Design: vault `architecture/Internal State.md`,
//! `decisions/ADR-0014 One store for internal state.md`.
//!
//! This crate is the **interface** (Milestone 18 phase 3), small enough for
//! the extension contract to depend on. The engines are in
//! `moonkale-state-stores`; SQLite is the app's (ADR-0014, the comparison in
//! `markdown/research/State Store Comparison.md`).
//!
//! ```text
//!   Typed (records)  ──►  StateStore (bytes)  ──►  MemoryStore | RedbStore | … (Turso, RocksDB, Helix, SQLite in phase 5)
//!     Record::TABLE, Key, versioned JSON          get · scan(prefix) · write(Batch)
//! ```
//!
//! Three operations are the whole contract: `get` one key, `scan` a key
//! prefix in key order, and `write` a [`Batch`] of puts and deletes across
//! tables **atomically**. Every candidate store has that shape (a redb or
//! SQLite transaction, a RocksDB `WriteBatch`), it needs no transaction
//! lifetimes, and it is object safe, so the app can hold `Arc<dyn StateStore>`.

mod copy;
mod key;
mod memory;
mod record;
pub mod tables;
pub mod testing;

pub use copy::copy;
pub use key::Key;
pub use memory::MemoryStore;
pub use record::{decode, encode, put_in, Record, Typed};

/// What can go wrong.
#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error("storage: {0}")]
    Storage(String),
    #[error("a stored {table} record could not be read: {message}")]
    Decode {
        table: &'static str,
        message: String,
    },
    #[error("a {table} record is version {found}, newer than this build reads ({supported})")]
    TooNew {
        table: &'static str,
        found: u32,
        supported: u32,
    },
}

/// How hard a backend works to keep a committed batch (Milestone 18 phase 5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Durability {
    /// Every commit is on disk when `write` returns (an fsync per commit):
    /// survives a power loss.
    #[default]
    Durable,
    /// Every commit survives the process being killed, but the last ones may
    /// be lost on a power loss (no fsync per commit). Backends without such a
    /// mode treat this as [`Durability::Durable`].
    Relaxed,
}

/// `(key, value)` pairs in key order, as [`StateStore::scan`] returns them.
pub type Entries = Vec<(Vec<u8>, Vec<u8>)>;

/// A set of changes applied all together or not at all.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Batch {
    pub ops: Vec<Op>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Op {
    Put {
        table: String,
        key: Vec<u8>,
        value: Vec<u8>,
    },
    Delete {
        table: String,
        key: Vec<u8>,
    },
}

impl Batch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn put(mut self, table: &str, key: impl Into<Vec<u8>>, value: impl Into<Vec<u8>>) -> Self {
        self.ops.push(Op::Put {
            table: table.to_string(),
            key: key.into(),
            value: value.into(),
        });
        self
    }

    pub fn delete(mut self, table: &str, key: impl Into<Vec<u8>>) -> Self {
        self.ops.push(Op::Delete {
            table: table.to_string(),
            key: key.into(),
        });
        self
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
}

/// The byte-level store every backend implements. Tables are created on
/// first write; reading a table that was never written is empty, not an
/// error.
pub trait StateStore: Send + Sync {
    /// The value under `key` in `table`.
    fn get(&self, table: &str, key: &[u8]) -> Result<Option<Vec<u8>>, StateError>;

    /// Every `(key, value)` in `table` whose key starts with `prefix`, in
    /// ascending key order (byte-wise). An empty prefix lists the table.
    fn scan(&self, table: &str, prefix: &[u8]) -> Result<Entries, StateError>;

    /// Apply every op of `batch` atomically, in order (a later op on the same
    /// key wins). Readers see the state before or after, never between.
    fn write(&self, batch: Batch) -> Result<(), StateError>;
}
