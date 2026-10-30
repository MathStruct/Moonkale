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
//! | `editor.replace` | mutating | queue one exact replacement as a versioned review proposal |
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
        t("editor.propose", "Queue a proposal across files and SQL rows for review. Optional rows entries contain source and typed batch; each database batch is atomic. SQL text is never accepted for writes. Nothing changes until the person approves from the session inbox. Each file requires the exact expected version string returned by graph.fetch. Approval changes all selected buffers atomically; files persist in order with a report.",
          json!({"rows":{"type":"array","maxItems":16,"items":{"type":"object","properties":{"source":{"type":"string"},"batch":{"type":"object","description":"RowBatch with typed RowOp entries from source.read_rows: table, key, expected, change (Update/Insert column maps or Delete)."}},"required":["source","batch"]}},"files":{"type":"array","minItems":0,"maxItems":64,"items":{"type":"object","properties":{"source":{"type":"string"},"node":{"type":"string"},"expected":{"type":"string"},"text":{"type":"string"}},"required":["source","node","expected","text"]}}}), &[]),
        t("source.read_rows", "Read up to 1000 authoritative SQL rows, including typed keys, values and opaque versions for editor.propose. Never infer keys from a raw query result.", json!({"source":{"type":"string"},"table":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":1000}}), &["source","table"]),
        t("ui.reveal", "Offer a node with its context in your marked group. Reveal a zero-based line/column range without taking focus.",
          json!({"source":{"type":"string"},"node":{"type":"string"},"line":{"type":"integer","minimum":0},"column":{"type":"integer","minimum":0},"end_line":{"type":"integer"},"end_column":{"type":"integer"}}), &["source","node"]),
        t("ui.select", "Select and fit nodes in your own group's graph. Uses the ids of the graph you last showed.", json!({"nodes":{"type":"array","maxItems":200,"items":{"type":"string"}}}), &["nodes"]),
        t("ui.show_result", "Show result rows or a subgraph. content is a typed table or graph, as for ui.canvas.", json!({"content":{"type":"object"}}), &["content"]),
        t("ui.canvas", "Show typed content in your marked group: content={kind:markdown,text:string}, {kind:table,columns:[string],rows:[[string]]}, or {kind:graph,nodes:[{id,label,style:default|accent|success|warning}],edges:[{from,to}]}. Maximum 64 KiB, 200 rows/nodes, 32 columns, 1000 edges. Never HTML, CSS, scripts or URLs as executable content.", json!({"content":{"type":"object"}}), &["content"]),
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
            "Offer a file or table in your marked link group without taking focus, subject to the person’s view-opening preference. Optional line is one-based (ui.reveal uses zero-based ranges).",
            json!({
                "source": { "type": "string" },
                "node": { "type": "string" },
                "line": { "type": "integer", "minimum": 1 }
            }),
            &["source", "node"],
        ),
        t(
            "editor.replace",
            "Queue a single-file review proposal replacing exactly one occurrence of `old` with `new`. `expected` must be the version string from graph.fetch. Include enough context for a unique exact match. Nothing changes until approval in the session inbox. Use editor.propose for changes reviewed together across files.",
            json!({
                "source": { "type": "string" },
                "expected": { "type": "string", "description": "version string from graph.fetch" },
                "node": { "type": "string", "description": "node id of the file" },
                "old": { "type": "string" },
                "new": { "type": "string" }
            }),
            &["source", "node", "expected", "old", "new"],
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
