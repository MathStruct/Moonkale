//! The gate. Every tool call is classified and a decision made before the
//! host runs it. Defaults: reads run; writes ask; destructive statements
//! always ask (and are refused outright when the source is read-only).

use crate::tools::ToolCall;
use moonkale_sources_sql::text::{classify, Statement};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Class {
    ReadOnly,
    Mutating,
    Destructive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Decision {
    Allow,
    Ask,
    Deny,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    /// Writes: `Ask` (default) or `Allow` (trusted session) or `Deny`.
    pub mutating: Decision,
    /// Destructive statements: `Ask` (default) or `Deny`.
    pub destructive: Decision,
    /// Tool names the agent may never call.
    pub denied_tools: Vec<String>,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            mutating: Decision::Ask,
            destructive: Decision::Ask,
            denied_tools: Vec::new(),
        }
    }
}

impl Policy {
    pub fn classify(call: &ToolCall) -> Class {
        match call.name.as_str() {
            "source.text_query" => {
                let text = call.str("text").unwrap_or_default();
                match call.str("dialect").unwrap_or("sql") {
                    "cypher" => classify_cypher(text),
                    _ => match classify(text) {
                        Statement::Read => Class::ReadOnly,
                        Statement::Write => Class::Mutating,
                        Statement::Ddl => Class::Destructive,
                        Statement::Unknown => Class::Mutating,
                    },
                }
            }
            // Everything else the surface exposes today only reads.
            _ => Class::ReadOnly,
        }
    }

    pub fn decide(&self, call: &ToolCall) -> (Class, Decision) {
        if self.denied_tools.iter().any(|t| t == &call.name) {
            return (Self::classify(call), Decision::Deny);
        }
        let class = Self::classify(call);
        let decision = match class {
            Class::ReadOnly => Decision::Allow,
            Class::Mutating => self.mutating,
            Class::Destructive => match self.destructive {
                Decision::Allow => Decision::Ask, // never silently
                d => d,
            },
        };
        (class, decision)
    }
}

/// Cypher has no statement classifier in the sources yet: keyword scan.
pub fn classify_cypher(text: &str) -> Class {
    let upper = text.to_ascii_uppercase();
    let has = |k: &str| {
        upper
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .any(|w| w == k)
    };
    if has("DROP") || (has("DELETE") && !has("WHERE")) || has("DETACH") || has("ALTER") {
        Class::Destructive
    } else if has("CREATE")
        || has("MERGE")
        || has("SET")
        || has("DELETE")
        || has("REMOVE")
        || has("COPY")
    {
        Class::Mutating
    } else {
        Class::ReadOnly
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn call(dialect: &str, text: &str) -> ToolCall {
        ToolCall {
            id: "1".into(),
            name: "source.text_query".into(),
            input: json!({ "source": "s", "dialect": dialect, "text": text }),
        }
    }

    #[test]
    fn sql_and_cypher_classes() {
        let p = Policy::default();
        assert_eq!(
            p.decide(&call("sql", "SELECT * FROM t")),
            (Class::ReadOnly, Decision::Allow)
        );
        assert_eq!(
            p.decide(&call("sql", "UPDATE t SET a = 1")),
            (Class::Mutating, Decision::Ask)
        );
        assert_eq!(
            p.decide(&call("sql", "DROP TABLE t")),
            (Class::Destructive, Decision::Ask)
        );
        assert_eq!(
            p.decide(&call("cypher", "MATCH (n) RETURN n")),
            (Class::ReadOnly, Decision::Allow)
        );
        assert_eq!(
            p.decide(&call("cypher", "CREATE (:Person {name:'x'})")),
            (Class::Mutating, Decision::Ask)
        );
        assert_eq!(
            p.decide(&call("cypher", "MATCH (n) DETACH DELETE n")),
            (Class::Destructive, Decision::Ask)
        );
    }

    #[test]
    fn denied_and_never_silent_destructive() {
        let p = Policy {
            destructive: Decision::Allow,
            denied_tools: vec!["editor.open".into()],
            ..Default::default()
        };
        assert_eq!(p.decide(&call("sql", "DROP TABLE t")).1, Decision::Ask);
        let open = ToolCall {
            id: "2".into(),
            name: "editor.open".into(),
            input: json!({}),
        };
        assert_eq!(p.decide(&open).1, Decision::Deny);
    }
}
