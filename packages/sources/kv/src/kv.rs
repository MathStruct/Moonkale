//! The shape every embedded key/value store shares (Milestone 17): a
//! database of named tables (redb tables, RocksDB column families), each a
//! sorted map of keys to values. [`KvStore`] is what a store implements;
//! [`KvSource`](crate::KvSource) turns one into a `Source`: the tables in the
//! Sources tree and the graph, and the `kv` text dialect in the table editor.
//!
//! The `kv` dialect:
//! - `tables`
//! - `scan <table> [prefix <p>] [limit <n>]` — keys in order, from the first
//!   one starting with `p`
//! - `get <table> <key>`
//!
//! Words with spaces or quotes are written in double quotes (`\"` and `\\`
//! escape). A key or prefix starting with `0x` is hex bytes. Keys and values
//! are shown as text when they are UTF-8 without control characters, else as
//! `0x…` hex.

/// One table of a store.
#[derive(Clone, Debug, PartialEq)]
pub struct KvTable {
    pub name: String,
    /// The stored key/value types (redb knows them; RocksDB is `bytes`).
    pub key_type: String,
    pub value_type: String,
    /// Number of entries, if the store knows it cheaply (an estimate for
    /// RocksDB).
    pub len: Option<u64>,
    /// `false`: listed but not readable here (a redb table of user-defined
    /// Rust types).
    pub readable: bool,
}

/// Rows as shown: rendered key and value, and whether more were available.
pub type Rows = (Vec<(String, String)>, bool);

/// A store that can be browsed. Blocking calls: `KvSource` runs them on
/// `spawn_blocking`.
pub trait KvStore: Send + Sync + 'static {
    fn tables(&self) -> Result<Vec<KvTable>, String>;
    /// Up to `limit` entries of `table` in key order, starting at the first
    /// key that starts with `prefix` and ending at the last one that does.
    fn scan(&self, table: &str, prefix: &[u8], limit: usize) -> Result<Rows, String>;
    /// One value by key.
    fn get(&self, table: &str, key: &[u8]) -> Result<Option<String>, String>;
}

/// A parsed `kv` statement.
#[derive(Clone, Debug, PartialEq)]
pub enum KvQuery {
    Tables,
    Scan {
        table: String,
        prefix: Vec<u8>,
        limit: usize,
    },
    Get {
        table: String,
        key: Vec<u8>,
    },
}

/// Most rows a scan returns, whatever `limit` says.
pub const MAX_ROWS: usize = 5000;
const DEFAULT_LIMIT: usize = 200;

/// Split a statement into words; double quotes group, `\` escapes inside them.
fn words(text: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut chars = text.trim().chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        let mut w = String::new();
        if c == '"' {
            chars.next();
            loop {
                match chars.next() {
                    None => return Err("unterminated quote".into()),
                    Some('"') => break,
                    Some('\\') => match chars.next() {
                        Some(e) => w.push(e),
                        None => return Err("unterminated quote".into()),
                    },
                    Some(o) => w.push(o),
                }
            }
        } else {
            while let Some(&o) = chars.peek() {
                if o.is_whitespace() {
                    break;
                }
                w.push(o);
                chars.next();
            }
        }
        out.push(w);
    }
    Ok(out)
}

/// `0x…` is hex bytes, anything else its UTF-8.
pub fn key_bytes(word: &str) -> Result<Vec<u8>, String> {
    match word.strip_prefix("0x") {
        Some(hex) if !hex.is_empty() => {
            if hex.len() % 2 != 0 {
                return Err(format!("odd number of hex digits in {word}"));
            }
            (0..hex.len())
                .step_by(2)
                .map(|i| {
                    u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| format!("not hex: {word}"))
                })
                .collect()
        }
        _ => Ok(word.as_bytes().to_vec()),
    }
}

pub fn parse(text: &str) -> Result<KvQuery, String> {
    let w = words(text)?;
    let usage = "kv: tables | scan <table> [prefix <p>] [limit <n>] | get <table> <key>";
    match w.first().map(|s| s.to_ascii_lowercase()).as_deref() {
        Some("tables") if w.len() == 1 => Ok(KvQuery::Tables),
        Some("get") if w.len() == 3 => Ok(KvQuery::Get {
            table: w[1].clone(),
            key: key_bytes(&w[2])?,
        }),
        Some("scan") if w.len() >= 2 => {
            let mut prefix = Vec::new();
            let mut limit = DEFAULT_LIMIT;
            let mut i = 2;
            while i < w.len() {
                match (w[i].to_ascii_lowercase().as_str(), w.get(i + 1)) {
                    ("prefix", Some(p)) => prefix = key_bytes(p)?,
                    ("limit", Some(n)) => {
                        limit = n.parse().map_err(|_| format!("limit: not a number: {n}"))?
                    }
                    _ => return Err(usage.into()),
                }
                i += 2;
            }
            Ok(KvQuery::Scan {
                table: w[1].clone(),
                prefix,
                limit: limit.clamp(1, MAX_ROWS),
            })
        }
        _ => Err(usage.into()),
    }
}

/// Bytes as text when they read as text, else `0x…` hex (capped).
pub fn show_bytes(b: &[u8]) -> String {
    match std::str::from_utf8(b) {
        Ok(s) if !s.chars().any(|c| c.is_control() && c != '\n' && c != '\t') => s.to_string(),
        _ => {
            let mut out = String::with_capacity(2 + b.len().min(512) * 2);
            out.push_str("0x");
            for x in b.iter().take(512) {
                out.push_str(&format!("{x:02x}"));
            }
            if b.len() > 512 {
                out.push_str(&format!("… ({} bytes)", b.len()));
            }
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_three_statements() {
        assert_eq!(parse("tables"), Ok(KvQuery::Tables));
        assert_eq!(
            parse("scan users"),
            Ok(KvQuery::Scan {
                table: "users".into(),
                prefix: vec![],
                limit: 200
            })
        );
        assert_eq!(
            parse(r#"SCAN "my table" prefix "a b" limit 5"#),
            Ok(KvQuery::Scan {
                table: "my table".into(),
                prefix: b"a b".to_vec(),
                limit: 5
            })
        );
        assert_eq!(
            parse("get t 0x00ff"),
            Ok(KvQuery::Get {
                table: "t".into(),
                key: vec![0, 255]
            })
        );
        assert!(parse("scan").is_err());
        assert!(parse("drop t").is_err());
        assert!(parse("get t 0xabc").is_err());
        assert_eq!(
            parse("scan t limit 999999").map(|q| match q {
                KvQuery::Scan { limit, .. } => limit,
                _ => 0,
            }),
            Ok(MAX_ROWS)
        );
    }

    #[test]
    fn shows_text_as_text_and_the_rest_as_hex() {
        assert_eq!(show_bytes(b"hello\nworld"), "hello\nworld");
        assert_eq!(show_bytes(&[0, 1, 255]), "0x0001ff");
        assert_eq!(show_bytes("ä".as_bytes()), "ä");
    }
}
