//! What the SQL backends (SQLite, Turso) share: the schema and the upper
//! bound of a prefix scan.

/// One table for every state table: `(t, k)` is the key; SQLite compares
/// BLOBs byte-wise, so `ORDER BY k` is the key order.
#[allow(dead_code)]
pub(crate) const SCHEMA: &str =
    "CREATE TABLE IF NOT EXISTS kv (t TEXT NOT NULL, k BLOB NOT NULL, v BLOB NOT NULL, PRIMARY KEY (t, k)) WITHOUT ROWID";

/// The same without `WITHOUT ROWID`, which Turso 0.7 only has behind an
/// experimental flag (found in the comparison, 2026-10-02).
#[allow(dead_code)]
pub(crate) const SCHEMA_ROWID: &str =
    "CREATE TABLE IF NOT EXISTS kv (t TEXT NOT NULL, k BLOB NOT NULL, v BLOB NOT NULL, PRIMARY KEY (t, k))";

/// The smallest key greater than every key starting with `prefix`, or
/// `None` when there is none (empty prefix, or all `0xFF`).
#[allow(dead_code)]
pub(crate) fn prefix_end(prefix: &[u8]) -> Option<Vec<u8>> {
    let mut end = prefix.to_vec();
    while let Some(last) = end.pop() {
        if last < 0xFF {
            end.push(last + 1);
            return Some(end);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn prefix_end_carries() {
        assert_eq!(super::prefix_end(b"ab"), Some(b"ac".to_vec()));
        assert_eq!(super::prefix_end(&[1, 0xFF]), Some(vec![2]));
        assert_eq!(super::prefix_end(&[0xFF]), None);
        assert_eq!(super::prefix_end(b""), None);
    }
}
