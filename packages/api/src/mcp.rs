//! MCP server (Milestone 5): the workspace's read-only tools for external
//! agents (Claude Code, IDE agents) over the streamable-HTTP transport's
//! JSON-only subset: `POST /mcp` with a JSON-RPC request → one JSON
//! response (no SSE stream, no sessions). Methods: `initialize`,
//! `notifications/initialized`, `ping`, `tools/list`, `tools/call`.
//!
//! Tools are the same [`moonkale_llm::builtin_tools`] definitions minus the
//! ones that need the UI (`editor.*`, `terminal.run`) or write
//! (`file.create`): an external agent gets **read-only** access to the
//! sources the server has open. Names use `_` instead of `.` because MCP
//! clients validate `^[a-zA-Z0-9_-]+$`.
//!
//! Auth: if `MOONKALE_MCP_TOKEN` is set, requests need
//! `Authorization: Bearer <token>`; otherwise the endpoint is open — dev
//! server only, like the terminal and LSP relays.

use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use moonkale_core::{Direction, NodeId, Query, SourceFamily, SourceId, TextDialect};
use moonkale_llm::tools::{cap, MAX_NODES, MAX_ROWS};
use moonkale_llm::ToolCall;
use serde_json::{json, Value};
use std::fmt::Write as _;

const PROTOCOL_VERSION: &str = "2025-03-26";
const READ_ONLY: &[&str] = &[
    "workspace.list_sources",
    "graph.query",
    "graph.fetch",
    "source.text_query",
    "index.search",
];

fn mcp_name(tool: &str) -> String {
    tool.replace('.', "_")
}

fn tool_name(mcp: &str) -> Option<&'static str> {
    READ_ONLY.iter().copied().find(|t| mcp_name(t) == mcp)
}

pub async fn handler(headers: HeaderMap, body: Json<Value>) -> Response {
    if let Ok(token) = std::env::var("MOONKALE_MCP_TOKEN") {
        let ok = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .is_some_and(|t| t == token);
        if !ok {
            return (StatusCode::UNAUTHORIZED, "missing or wrong bearer token").into_response();
        }
    }
    let req = body.0;
    let id = req.get("id").cloned();
    let method = req["method"].as_str().unwrap_or_default().to_string();
    let params = req.get("params").cloned().unwrap_or(json!({}));
    // Notifications have no id and get no body.
    if id.is_none() {
        return StatusCode::ACCEPTED.into_response();
    }
    let result: Result<Value, (i64, String)> = match method.as_str() {
        "initialize" => Ok(json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": { "name": "moonkale", "version": env!("CARGO_PKG_VERSION") },
            "instructions": "Read-only access to the sources open in this Moonkale server. Call workspace_list_sources first for ids."
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({
            "tools": moonkale_llm::builtin_tools()
                .into_iter()
                .filter(|t| READ_ONLY.contains(&t.name.as_str()))
                .map(|t| json!({ "name": mcp_name(&t.name), "description": t.description, "inputSchema": t.input_schema }))
                .collect::<Vec<_>>()
        })),
        "tools/call" => {
            let name = params["name"].as_str().unwrap_or_default();
            match tool_name(name) {
                None => Err((-32602, format!("unknown tool {name}"))),
                Some(tool) => {
                    let call = ToolCall {
                        id: "mcp".into(),
                        name: tool.into(),
                        input: params.get("arguments").cloned().unwrap_or(json!({})),
                    };
                    match run(call).await {
                        Ok(text) => Ok(
                            json!({ "content": [{ "type": "text", "text": cap(text) }], "isError": false }),
                        ),
                        Err(e) => Ok(
                            json!({ "content": [{ "type": "text", "text": e }], "isError": true }),
                        ),
                    }
                }
            }
        }
        _ => Err((-32601, format!("method {method} not supported"))),
    };
    let body = match result {
        Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
        Err((code, message)) => {
            json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
        }
    };
    Json(body).into_response()
}

/// The read-only tools over the server's registry (mirrors the in-app
/// `WorkspaceHost`, without a workspace).
pub(crate) async fn run(call: ToolCall) -> Result<String, String> {
    let reg = crate::state::registry();
    let source_of = |call: &ToolCall| -> Result<std::sync::Arc<dyn moonkale_core::Source>, String> {
        let id = call.str("source").ok_or("source is required")?;
        reg.get(&SourceId::new(id))
            .ok_or_else(|| format!("unknown source {id}; call workspace_list_sources"))
    };
    match call.name.as_str() {
        "workspace.list_sources" => {
            let mut out = String::from("id\tname\tfamily\tdialect\troot\n");
            for s in reg.all() {
                let d = s.descriptor();
                let dialect = match d.capabilities.text_query {
                    Some(TextDialect::Sql) => "sql",
                    Some(TextDialect::Cypher) => "cypher",
                    Some(TextDialect::TypeQl) => "typeql",
                    None => match d.family {
                        SourceFamily::Index => "search",
                        _ => "-",
                    },
                };
                let _ = writeln!(
                    out,
                    "{}\t{}\t{:?}\t{}\t{}",
                    d.id, d.display_name, d.family, dialect, d.root
                );
            }
            Ok(out)
        }
        "graph.query" => {
            let source = source_of(&call)?;
            let node = match call.str("node") {
                Some(n) => n
                    .parse::<NodeId>()
                    .map_err(|_| format!("{n} is not a node id"))?,
                None => source.descriptor().root,
            };
            let limit = call.u64("limit").unwrap_or(MAX_NODES as u64) as usize;
            let query = match call.str("mode").unwrap_or("children") {
                "neighbours" => Query::Neighbours {
                    node,
                    depth: call.u64("depth").unwrap_or(1).clamp(1, 3) as u32,
                    direction: Direction::Both,
                },
                "all" => Query::All {
                    limit: limit.min(MAX_NODES),
                    kinds: None,
                },
                _ => Query::Children(node),
            };
            let res = source.query(query).await.map_err(|e| e.to_string())?;
            let mut out = format!(
                "{} nodes, {} edges{}\nid\tkind\tlabel\tkey\n",
                res.nodes.len(),
                res.edges.len(),
                if res.truncated { " (truncated)" } else { "" }
            );
            for n in res.nodes.iter().take(limit.min(MAX_NODES)) {
                let _ = writeln!(out, "{}\t{:?}\t{}\t{}", n.id, n.kind, n.label, n.native_key);
            }
            Ok(out)
        }
        "graph.fetch" => {
            let source = source_of(&call)?;
            let node = call
                .str("node")
                .ok_or("node is required")?
                .parse::<NodeId>()
                .map_err(|_| "bad node id".to_string())?;
            let (text, _) = source.fetch_text(node).await.map_err(|e| e.to_string())?;
            let offset = call.u64("offset").unwrap_or(0) as usize;
            let max = call.u64("max_chars").unwrap_or(8000) as usize;
            let total = text.chars().count();
            let slice: String = text.chars().skip(offset).take(max).collect();
            Ok(format!(
                "[chars {}..{} of {}]\n{}",
                offset,
                (offset + slice.chars().count()).min(total),
                total,
                slice
            ))
        }
        "source.text_query" | "index.search" => {
            let (source, dialect, text) = if call.name == "index.search" {
                let index = reg
                    .all()
                    .into_iter()
                    .find(|s| s.descriptor().family == SourceFamily::Index)
                    .ok_or("no folder is open on the server (no index)")?;
                (
                    index,
                    "search".to_string(),
                    call.str("query").unwrap_or_default().to_string(),
                )
            } else {
                let dialect = call.str("dialect").unwrap_or("sql").to_string();
                let text = call.str("text").unwrap_or_default().to_string();
                let tc = ToolCall {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    input: call.input.clone(),
                };
                // External agents are read-only: refuse anything the policy
                // would not run silently.
                let (class, _) = moonkale_llm::Policy::default().decide(&tc);
                if class != moonkale_llm::Class::ReadOnly {
                    return Err(format!("{class:?} statements are not allowed over MCP"));
                }
                (source_of(&call)?, dialect, text)
            };
            let res = source
                .query(Query::Text { dialect, text })
                .await
                .map_err(|e| e.to_string())?;
            let mut out = String::new();
            if let Some(t) = &res.table {
                let _ = writeln!(
                    out,
                    "{} rows{}",
                    t.rows.len(),
                    if t.truncated { " (truncated)" } else { "" }
                );
                let _ = writeln!(out, "{}", t.columns.join("\t"));
                for row in t.rows.iter().take(MAX_ROWS) {
                    let cells: Vec<String> = row.iter().map(|v| v.to_string()).collect();
                    let _ = writeln!(out, "{}", cells.join("\t"));
                }
            }
            if !res.nodes.is_empty() {
                let _ = writeln!(out, "{} nodes, {} edges:", res.nodes.len(), res.edges.len());
                for n in res.nodes.iter().take(MAX_NODES) {
                    let _ = writeln!(out, "{}\t{:?}\t{}\t{}", n.id, n.kind, n.label, n.native_key);
                }
            }
            Ok(if out.is_empty() {
                "(empty result)".into()
            } else {
                out
            })
        }
        other => Err(format!("{other} is not available over MCP")),
    }
}
