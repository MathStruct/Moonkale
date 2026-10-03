//! The tool surface: what an agent can do, described as JSON-Schema tools.
//! Execution is the host's job ([`crate::agent::ToolHost`]); this module
//! only defines the built-ins and parses calls.
//!
//! | tool | class | what |
//! |---|---|---|
//! | `workspace.list_sources` | read | open sources: id, name, family, dialect |
//! | `graph.query` | read | `children` / `neighbours` / `all` on a source; ids + labels |
//! | `graph.fetch` | read | a node's text (capped) |
//! | `source.text_query` | classified | raw SQL / Cypher / search on a source |
//! | `index.search` | read | hybrid search over the index |
//! | `editor.open` | read | open a node in the editor (optionally at a line) |
//! | `editor.replace` | mutating | replace one exact occurrence of text in an open document (the user still saves) |
//! | `file.create` | mutating | create a new text file in a folder |
//! | `terminal.run` | mutating (destructive by pattern) | run a shell command in a new terminal and return its output |

use crate::types::ToolDef;
use serde_json::{json, Value};

/// A parsed call from the model.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: Value,
}

impl ToolCall {
    pub fn str(&self, key: &str) -> Option<&str> {
        self.input.get(key).and_then(Value::as_str)
    }
    pub fn u64(&self, key: &str) -> Option<u64> {
        self.input.get(key).and_then(Value::as_u64)
    }
}

pub fn builtin_tools() -> Vec<ToolDef> {
    let t = |name: &str, description: &str, props: Value, required: &[&str]| ToolDef {
        name: name.into(),
        description: description.into(),
        input_schema: json!({ "type": "object", "properties": props, "required": required }),
    };
    vec![
        t(
            "workspace.list_sources",
            "List the open sources (folders, indexes, databases) with their ids, display names, families and query dialects. Call this first to learn what ids to use.",
            json!({}),
            &[],
        ),
        t(
            "graph.query",
            "Walk a source's graph. mode=children lists what a node contains (folder → files, database → tables); mode=neighbours lists linked nodes (backlinks, symbols, relations) up to `depth`; mode=all lists every node (capped). Omit `node` for the source root. Returns ids, kinds and labels — use graph.fetch for text.",
            json!({
                "source": { "type": "string", "description": "source id from workspace.list_sources" },
                "mode": { "type": "string", "enum": ["children", "neighbours", "all"] },
                "node": { "type": "string", "description": "node id (default: the source root)" },
                "depth": { "type": "integer", "minimum": 1, "maximum": 3 },
                "limit": { "type": "integer", "minimum": 1, "maximum": 500 }
            }),
            &["source", "mode"],
        ),
        t(
            "graph.fetch",
            "Read a text node (a file or page). Returns up to `max_chars` characters (default 8000) starting at `offset`.",
            json!({
                "source": { "type": "string" },
                "node": { "type": "string", "description": "node id" },
                "offset": { "type": "integer", "minimum": 0 },
                "max_chars": { "type": "integer", "minimum": 1, "maximum": 50000 }
            }),
            &["source", "node"],
        ),
        t(
            "source.text_query",
            "Run a query in the source's own language: SQL on SQL sources, Cypher on graph databases. Read statements run directly; anything that writes needs the user's approval.",
            json!({
                "source": { "type": "string" },
                "dialect": { "type": "string", "enum": ["sql", "cypher"] },
                "text": { "type": "string" }
            }),
            &["source", "dialect", "text"],
        ),
        t(
            "index.search",
            "Search the open folder's files (hybrid keyword + semantic). Returns the best matching chunks with file paths and line numbers.",
            json!({
                "query": { "type": "string" },
                "limit": { "type": "integer", "minimum": 1, "maximum": 50 }
            }),
            &["query"],
        ),
        t(
            "editor.open",
            "Open a file or table for the user in the editor, optionally at a line.",
            json!({
                "source": { "type": "string" },
                "node": { "type": "string" },
                "line": { "type": "integer", "minimum": 1 }
            }),
            &["source", "node"],
        ),
        t(
            "editor.replace",
            "Edit a text file: replace exactly one occurrence of `old` with `new` in the document (opened in the editor if needed). `old` must match the file text exactly, including whitespace; include enough context to be unique. The change appears in the user's editor as an unsaved edit; the user saves.",
            json!({
                "source": { "type": "string" },
                "node": { "type": "string", "description": "node id of the file" },
                "old": { "type": "string" },
                "new": { "type": "string" }
            }),
            &["source", "node", "old", "new"],
        ),
        t(
            "file.create",
            "Create a new text file at a relative path inside a folder source (missing directories are created; existing files are never overwritten).",
            json!({
                "source": { "type": "string", "description": "a folder source id" },
                "path": { "type": "string", "description": "relative path, e.g. notes/todo.md" },
                "text": { "type": "string" }
            }),
            &["source", "path", "text"],
        ),
        t(
            "terminal.run",
            "Run a shell command in a new terminal (visible to the user) and return its output. Use for builds, tests and git. Non-interactive commands only; the shell exits after the command.",
            json!({
                "command": { "type": "string" },
                "cwd": { "type": "string", "description": "working directory (default: the open folder)" }
            }),
            &["command"],
        ),
    ]
}

/// Caps that keep a tool result from flooding the context window.
pub const MAX_RESULT_CHARS: usize = 12_000;
pub const MAX_ROWS: usize = 100;
pub const MAX_NODES: usize = 200;

/// Cut a result to [`MAX_RESULT_CHARS`], marking the cut.
pub fn cap(text: String) -> String {
    if text.chars().count() <= MAX_RESULT_CHARS {
        return text;
    }
    let mut out: String = text.chars().take(MAX_RESULT_CHARS).collect();
    out.push_str("\n… [truncated]");
    out
}
