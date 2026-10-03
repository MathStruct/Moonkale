//! redb (Milestone 17): a single-file, pure-Rust ACID key/value store with
//! typed tables.
//!
//! Opened with `ReadOnlyDatabase`. redb tables are typed (`TableDefinition<K,
//! V>`) and opening one with other types fails with `TableTypeMismatch` —
//! which names the stored types, so each table is probed once and then read
//! with the matching built-in types. Tables of user-defined Rust types are
//! listed with their type names but cannot be read here.

use crate::kv::{show_bytes, KvStore, KvTable, Rows};
use redb::{
    ReadOnlyDatabase, ReadableDatabase, ReadableTable, ReadableTableMetadata, TableDefinition,
    TableError, TableHandle,
};
use std::path::Path;

pub struct RedbStore {
    db: ReadOnlyDatabase,
}

impl RedbStore {
    pub fn open(path: &Path) -> Result<Self, String> {
        let db = ReadOnlyDatabase::open(path).map_err(|e| e.to_string())?;
        Ok(Self { db })
    }

    /// The stored key/value type names of a table.
    fn types_of(&self, name: &str) -> Result<(String, String), String> {
        let txn = self.db.begin_read().map_err(|e| e.to_string())?;
        match txn.open_table(TableDefinition::<&[u8], &[u8]>::new(name)) {
            Ok(_) => Ok(("&[u8]".into(), "&[u8]".into())),
            Err(TableError::TableTypeMismatch { key, value, .. }) => {
                Ok((key.name().to_string(), value.name().to_string()))
            }
            Err(e) => Err(e.to_string()),
        }
    }
}

/// How a stored value is shown: string and byte types by their bytes, the
/// rest by `Debug` (numbers, bools).
fn show<T: redb::Value>(v: &T::SelfType<'_>, by_bytes: bool) -> String {
    if by_bytes {
        show_bytes(T::as_bytes(v).as_ref())
    } else {
        format!("{v:?}")
    }
}

fn by_bytes(type_name: &str) -> bool {
    matches!(type_name, "&str" | "String" | "&[u8]" | "Vec<u8>") || type_name.starts_with("[u8;")
}

/// Read one table as `K → V`. The prefix is matched against the key as
/// shown (a string key by its text, a number by its digits).
fn scan_as<K: redb::Key + 'static, V: redb::Value + 'static>(
    db: &ReadOnlyDatabase,
    name: &str,
    prefix: &[u8],
    limit: usize,
    get: Option<&[u8]>,
) -> Result<Rows, String> {
    let kb = by_bytes(K::type_name().name());
    let vb = by_bytes(V::type_name().name());
    let prefix = show_bytes(prefix);
    let want = get.map(show_bytes);
    let txn = db.begin_read().map_err(|e| e.to_string())?;
    let table = txn
        .open_table(TableDefinition::<K, V>::new(name))
        .map_err(|e| e.to_string())?;
    let mut rows = Vec::new();
    let mut started = false;
    for item in table.iter().map_err(|e| e.to_string())? {
        let (k, v) = item.map_err(|e| e.to_string())?;
        let key = show::<K>(&k.value(), kb);
        if let Some(w) = &want {
            if &key == w {
                return Ok((vec![(key, show::<V>(&v.value(), vb))], false));
            }
            continue;
        }
        if !key.starts_with(&prefix) {
            // Keys are sorted: past the prefix block means done. (For
            // numeric keys the shown order differs; keep scanning then.)
            if started && kb {
                break;
            }
            continue;
        }
        started = true;
        if rows.len() == limit {
            return Ok((rows, true));
        }
        rows.push((key, show::<V>(&v.value(), vb)));
    }
    Ok((rows, false))
}

/// Dispatch a (key type, value type) pair of names to `scan_as` with the
/// matching Rust types; `None` for types this build cannot read.
macro_rules! dispatch_value {
    ($k:ty, $vname:expr, $($args:expr),*) => {
        match $vname {
            "&str" => Some(scan_as::<$k, &str>($($args),*)),
            "String" => Some(scan_as::<$k, String>($($args),*)),
            "&[u8]" => Some(scan_as::<$k, &[u8]>($($args),*)),
            "Vec<u8>" => Some(scan_as::<$k, Vec<u8>>($($args),*)),
            "u64" => Some(scan_as::<$k, u64>($($args),*)),
            "i64" => Some(scan_as::<$k, i64>($($args),*)),
            "u32" => Some(scan_as::<$k, u32>($($args),*)),
            "i32" => Some(scan_as::<$k, i32>($($args),*)),
            "f64" => Some(scan_as::<$k, f64>($($args),*)),
            "bool" => Some(scan_as::<$k, bool>($($args),*)),
            "()" => Some(scan_as::<$k, ()>($($args),*)),
            _ => None,
        }
    };
}

fn dispatch(
    db: &ReadOnlyDatabase,
    (kname, vname): (&str, &str),
    name: &str,
    prefix: &[u8],
    limit: usize,
    get: Option<&[u8]>,
) -> Option<Result<Rows, String>> {
    match kname {
        "&str" => dispatch_value!(&str, vname, db, name, prefix, limit, get),
        "String" => dispatch_value!(String, vname, db, name, prefix, limit, get),
        "&[u8]" => dispatch_value!(&[u8], vname, db, name, prefix, limit, get),
        "u64" => dispatch_value!(u64, vname, db, name, prefix, limit, get),
        "i64" => dispatch_value!(i64, vname, db, name, prefix, limit, get),
        "u32" => dispatch_value!(u32, vname, db, name, prefix, limit, get),
        "i32" => dispatch_value!(i32, vname, db, name, prefix, limit, get),
        "u128" => dispatch_value!(u128, vname, db, name, prefix, limit, get),
        _ => None,
    }
}

impl KvStore for RedbStore {
    fn tables(&self) -> Result<Vec<KvTable>, String> {
        let txn = self.db.begin_read().map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for handle in txn.list_tables().map_err(|e| e.to_string())? {
            let name = handle.name().to_string();
            let len = txn
                .open_untyped_table(handle)
                .ok()
                .and_then(|t| t.len().ok());
            let (key_type, value_type) = self.types_of(&name)?;
            let readable = dispatch_readable(&key_type, &value_type);
            out.push(KvTable {
                name,
                key_type,
                value_type,
                len,
                readable,
            });
        }
        Ok(out)
    }

    fn scan(&self, table: &str, prefix: &[u8], limit: usize) -> Result<Rows, String> {
        let (k, v) = self.types_of(table)?;
        dispatch(&self.db, (&k, &v), table, prefix, limit, None)
            .unwrap_or_else(|| Err(format!("{table}: {k} → {v} cannot be read here")))
    }

    fn get(&self, table: &str, key: &[u8]) -> Result<Option<String>, String> {
        let (k, v) = self.types_of(table)?;
        let (rows, _) = dispatch(&self.db, (&k, &v), table, &[], 1, Some(key))
            .unwrap_or_else(|| Err(format!("{table}: {k} → {v} cannot be read here")))?;
        Ok(rows.into_iter().next().map(|(_, v)| v))
    }
}

fn dispatch_readable(k: &str, v: &str) -> bool {
    const KEYS: &[&str] = &[
        "&str", "String", "&[u8]", "u64", "i64", "u32", "i32", "u128",
    ];
    const VALUES: &[&str] = &[
        "&str", "String", "&[u8]", "Vec<u8>", "u64", "i64", "u32", "i32", "f64", "bool", "()",
    ];
    KEYS.contains(&k) && VALUES.contains(&v)
}
