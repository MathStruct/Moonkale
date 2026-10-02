//! Moving the state from one backend to another (ADR-0014: SQLite now, and
//! a different engine later stays a copy, not a rewrite). Every record is a
//! versioned envelope behind the same `StateStore`, so a migration is: scan
//! each table of the old store, write it to the new one in batches.

use crate::{Batch, StateError, StateStore};

/// How many entries go into one batch while copying.
const CHUNK: usize = 1000;

/// Copy every entry of `tables` from `from` into `to`; returns how many
/// entries were copied. Entries already in `to` under the same key are
/// overwritten; nothing is deleted. Use [`crate::tables::ALL`] for
/// Moonkale's tables.
pub fn copy(
    from: &dyn StateStore,
    to: &dyn StateStore,
    tables: &[&str],
) -> Result<usize, StateError> {
    let mut n = 0;
    for table in tables {
        let entries = from.scan(table, b"")?;
        for chunk in entries.chunks(CHUNK) {
            let mut batch = Batch::new();
            for (k, v) in chunk {
                batch = batch.put(table, k.clone(), v.clone());
            }
            to.write(batch)?;
            n += chunk.len();
        }
    }
    Ok(n)
}
