//! Statement classification: the read-only gate for `Query::Text`, shared by
//! every SQL dialect and reused by the LLM policy layer later.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Statement {
    Read,
    Write,
    Ddl,
    Unknown,
}

/// Classify by the first keyword after comments/whitespace. Conservative:
/// anything not recognised as a read is refused by read-only sources.
pub fn classify(sql: &str) -> Statement {
    let s = strip_comments(sql.trim_start());
    let word: String = s
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect::<String>()
        .to_ascii_uppercase();
    match word.as_str() {
        "SELECT" => Statement::Read,
        // A CTE can wrap a write: `WITH t AS (SELECT 1) INSERT INTO x VALUES
        // (1)`. Classify the WITH statement as a read only when no
        // data-modifying keyword appears outside string literals.
        "WITH" => {
            if contains_write_keyword(&s) {
                Statement::Write
            } else {
                Statement::Read
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
                "TABLE_INFO", "TABLE_LIST", "TABLE_XINFO", "INDEX_INFO", "INDEX_LIST",
                "INDEX_XINFO", "DATABASE_LIST", "COLLATION_LIST", "COMPILE_OPTIONS",
                "FUNCTION_LIST", "MODULE_LIST", "PRAGMA_LIST", "INTEGRITY_CHECK",
                "QUICK_CHECK", "ENCODING", "PAGE_COUNT", "PAGE_SIZE",
                "FOREIGN_KEY_LIST", "APPLICATION_ID",
            ];
            if READ_PRAGMAS.contains(&name.as_str()) && !s.contains('=') {
                Statement::Read
            } else {
                Statement::Unknown
            }
        }
        "EXPLAIN" | "VALUES" | "SHOW" | "DESCRIBE" => Statement::Read,
        "INSERT" | "UPDATE" | "DELETE" | "REPLACE" | "MERGE" => Statement::Write,
        "CREATE" | "DROP" | "ALTER" | "TRUNCATE" | "ATTACH" | "DETACH" | "VACUUM" => {
            Statement::Ddl
        }
        _ => Statement::Unknown,
    }
}

fn strip_comments(mut s: &str) -> &str {
    loop {
        if let Some(rest) = s.strip_prefix("--") {
            s = rest.split_once('\n').map(|(_, r)| r).unwrap_or("").trim_start();
        } else if let Some(rest) = s.strip_prefix("/*") {
            s = rest.split_once("*/").map(|(_, r)| r).unwrap_or("").trim_start();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_after_comments() {
        assert_eq!(classify("  -- hi\n /* x */ select 1"), Statement::Read);
        assert_eq!(
            classify("WITH t AS (SELECT 1) SELECT * FROM t"),
            Statement::Read
        );
        assert_eq!(classify("delete from a"), Statement::Write);
        assert_eq!(classify("DROP TABLE a"), Statement::Ddl);
        assert_eq!(classify(""), Statement::Unknown);
    }
    #[test]
    fn cte_wrapped_writes_are_not_reads() {
        assert_eq!(
            classify("WITH t AS (SELECT 1) INSERT INTO x VALUES (1)"),
            Statement::Write
        );
        assert_eq!(
            classify("WITH t AS (SELECT 1) DELETE FROM x WHERE id = 1"),
            Statement::Write
        );
        assert_eq!(
            classify("WITH t AS (SELECT 1) UPDATE x SET a = 1"),
            Statement::Write
        );
        // A read that merely mentions write keywords in literals stays a read.
        assert_eq!(
            classify("WITH t AS (SELECT 'delete me' AS w) SELECT * FROM t"),
            Statement::Read
        );
    }

    #[test]
    fn pragmas_are_whitelisted_by_name() {
        assert_eq!(classify("PRAGMA table_info(t)"), Statement::Read);
        assert_eq!(classify("PRAGMA writable_schema=1"), Statement::Unknown);
        assert_eq!(classify("PRAGMA journal_mode=WAL"), Statement::Unknown);
    }
}
