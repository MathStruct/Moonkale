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
    let mut s = sql.trim_start();
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
    let word: String = s
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect::<String>()
        .to_ascii_uppercase();
    match word.as_str() {
        "SELECT" | "WITH" | "PRAGMA" | "EXPLAIN" | "VALUES" | "SHOW" | "DESCRIBE" => {
            Statement::Read
        }
        "INSERT" | "UPDATE" | "DELETE" | "REPLACE" | "MERGE" => Statement::Write,
        "CREATE" | "DROP" | "ALTER" | "TRUNCATE" | "ATTACH" | "DETACH" | "VACUUM" => Statement::Ddl,
        _ => Statement::Unknown,
    }
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
}
