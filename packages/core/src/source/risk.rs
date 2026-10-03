//! How risky a text query is — the read-only gate of the sources and the
//! agent's policy share this (Milestone 18 phase 2: it moved here from
//! `moonkale-sources-sql::text` and `moonkale-llm::policy` so that the policy
//! asks the *source* instead of depending on a driver crate).
//!
//! Pure string functions, no I/O: they compile everywhere, so a web client's
//! `RemoteSource` classifies locally with the same rules the server uses.
//! [`crate::Source::classify`] calls [`classify`] by default; a source with
//! better knowledge overrides it.

use serde::{Deserialize, Serialize};

/// What running a text query would do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Risk {
    /// Only reads.
    Read,
    /// Changes data.
    Write,
    /// Changes or removes structure, or deletes wholesale (DDL, `DETACH
    /// DELETE`, `DELETE` without `WHERE`).
    Destructive,
    /// Not recognised — treat as a write.
    Unknown,
}

/// The default classification by dialect name (`Query::Text { dialect }`).
/// `kv` (`scan`/`get`) and `helix` (`nodes`/`edges`) only have reading verbs.
pub fn classify(dialect: &str, text: &str) -> Risk {
    match dialect {
        "sql" => classify_sql(text),
        "cypher" => classify_cypher(text),
        "kv" | "helix" => Risk::Read,
        _ => Risk::Unknown,
    }
}

/// SQL: by the first keyword after comments/whitespace. Conservative:
/// anything not recognised as a read is refused by read-only sources.
pub fn classify_sql(sql: &str) -> Risk {
    let s = strip_comments(sql.trim_start());
    let word: String = s
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect::<String>()
        .to_ascii_uppercase();
    match word.as_str() {
        "SELECT" => Risk::Read,
        // A CTE can wrap a write: `WITH t AS (SELECT 1) INSERT INTO x VALUES
        // (1)`. Classify the WITH statement as a read only when no
        // data-modifying keyword appears outside string literals.
        "WITH" => {
            if contains_write_keyword(s) {
                Risk::Write
            } else {
                Risk::Read
            }
        }
        // PRAGMAs are only reads for the introspection names we use; an
        // unrecognised (or value-setting) pragma is refused.
        "PRAGMA" => {
            let name: String = s["PRAGMA".len()..]
                .trim_start()
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect::<String>()
                .to_ascii_uppercase();
            const READ_PRAGMAS: &[&str] = &[
                "TABLE_INFO",
                "TABLE_LIST",
                "TABLE_XINFO",
                "INDEX_INFO",
                "INDEX_LIST",
                "INDEX_XINFO",
                "DATABASE_LIST",
                "COLLATION_LIST",
                "COMPILE_OPTIONS",
                "FUNCTION_LIST",
                "MODULE_LIST",
                "PRAGMA_LIST",
                "INTEGRITY_CHECK",
                "QUICK_CHECK",
                "ENCODING",
                "PAGE_COUNT",
                "PAGE_SIZE",
                "FOREIGN_KEY_LIST",
                "APPLICATION_ID",
            ];
            if READ_PRAGMAS.contains(&name.as_str()) && !s.contains('=') {
                Risk::Read
            } else {
                Risk::Unknown
            }
        }
        "EXPLAIN" | "VALUES" | "SHOW" | "DESCRIBE" => Risk::Read,
        "INSERT" | "UPDATE" | "DELETE" | "REPLACE" | "MERGE" => Risk::Write,
        "CREATE" | "DROP" | "ALTER" | "TRUNCATE" | "ATTACH" | "DETACH" | "VACUUM" => {
            Risk::Destructive
        }
        _ => Risk::Unknown,
    }
}

fn strip_comments(mut s: &str) -> &str {
    loop {
        if let Some(rest) = s.strip_prefix("--") {
            s = rest
                .split_once('\n')
                .map(|(_, r)| r)
                .unwrap_or("")
                .trim_start();
        } else if let Some(rest) = s.strip_prefix("/*") {
            s = rest
                .split_once("*/")
                .map(|(_, r)| r)
                .unwrap_or("")
                .trim_start();
        } else {
            break;
        }
    }
    s
}

/// True when a data-modifying keyword appears as a standalone word outside
/// string literals. Conservative by design: a false positive only refuses a
/// query, a false negative would let a write through the gate.
fn contains_write_keyword(s: &str) -> bool {
    let mut word = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' | '"' => {
                word.clear();
                // Skip to the closing quote (doubled quote is an escape).
                while let Some(q) = chars.next() {
                    if q == c {
                        if chars.peek() == Some(&c) {
                            chars.next();
                        } else {
                            break;
                        }
                    }
                }
            }
            c if c.is_ascii_alphabetic() => word.push(c.to_ascii_uppercase()),
            _ => {
                if matches!(
                    word.as_str(),
                    "INSERT" | "UPDATE" | "DELETE" | "REPLACE" | "MERGE"
                ) {
                    return true;
                }
                word.clear();
            }
        }
    }
    matches!(
        word.as_str(),
        "INSERT" | "UPDATE" | "DELETE" | "REPLACE" | "MERGE"
    )
}

/// Cypher: a keyword scan (LadybugDB, later FalkorDB).
pub fn classify_cypher(text: &str) -> Risk {
    let upper = text.to_ascii_uppercase();
    let has = |k: &str| {
        upper
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .any(|w| w == k)
    };
    if has("DROP") || (has("DELETE") && !has("WHERE")) || has("DETACH") || has("ALTER") {
        Risk::Destructive
    } else if has("CREATE")
        || has("MERGE")
        || has("SET")
        || has("DELETE")
        || has("REMOVE")
        || has("COPY")
    {
        Risk::Write
    } else {
        Risk::Read
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_after_comments() {
        assert_eq!(classify_sql("  -- hi\n /* x */ select 1"), Risk::Read);
        assert_eq!(
            classify_sql("WITH t AS (SELECT 1) SELECT * FROM t"),
            Risk::Read
        );
        assert_eq!(classify_sql("delete from a"), Risk::Write);
        assert_eq!(classify_sql("DROP TABLE a"), Risk::Destructive);
        assert_eq!(classify_sql(""), Risk::Unknown);
    }
    #[test]
    fn cte_wrapped_writes_are_not_reads() {
        assert_eq!(
            classify_sql("WITH t AS (SELECT 1) INSERT INTO x VALUES (1)"),
            Risk::Write
        );
        assert_eq!(
            classify_sql("WITH t AS (SELECT 1) DELETE FROM x WHERE id = 1"),
            Risk::Write
        );
        assert_eq!(
            classify_sql("WITH t AS (SELECT 1) UPDATE x SET a = 1"),
            Risk::Write
        );
        // A read that merely mentions write keywords in literals stays a read.
        assert_eq!(
            classify_sql("WITH t AS (SELECT 'delete me' AS w) SELECT * FROM t"),
            Risk::Read
        );
    }

    #[test]
    fn pragmas_are_whitelisted_by_name() {
        assert_eq!(classify_sql("PRAGMA table_info(t)"), Risk::Read);
        assert_eq!(classify_sql("PRAGMA writable_schema=1"), Risk::Unknown);
        assert_eq!(classify_sql("PRAGMA journal_mode=WAL"), Risk::Unknown);
    }

    #[test]
    fn cypher_and_dispatch() {
        assert_eq!(classify_cypher("MATCH (n) RETURN n"), Risk::Read);
        assert_eq!(classify_cypher("CREATE (:Person {name:'x'})"), Risk::Write);
        assert_eq!(
            classify_cypher("MATCH (n) DETACH DELETE n"),
            Risk::Destructive
        );
        assert_eq!(classify("sql", "DROP TABLE t"), Risk::Destructive);
        assert_eq!(classify("kv", "scan t"), Risk::Read);
        assert_eq!(classify("typeql", "match $x;"), Risk::Unknown);
    }
}
