//! Tool execution against the workspace, and the approval hand-off to the UI.

use dioxus::prelude::*;
use moonkale_core::{Direction, NodeId, Query, SourceFamily, TextDialect};
use moonkale_ext_api::Workspace;
use moonkale_llm::agent::HostFuture;
use moonkale_llm::tools::{MAX_NODES, MAX_ROWS};
use moonkale_llm::{Class, ToolCall, ToolHost};
use std::fmt::Write as _;

/// A call waiting for the user's yes/no; the panel renders it.
#[derive(Clone)]
pub struct PendingApproval {
    pub call: ToolCall,
    pub class: Class,
    pub reply: std::rc::Rc<std::cell::RefCell<Option<futures_channel::oneshot::Sender<bool>>>>,
}

impl PartialEq for PendingApproval {
    fn eq(&self, other: &Self) -> bool {
        self.call == other.call
    }
}

pub struct WorkspaceHost {
    pub ws: Workspace,
    pub pending: Signal<Option<PendingApproval>>,
    /// Relative paths the agent touched (for transcript wiki-links).
    pub cited: Signal<Vec<String>>,
}

impl WorkspaceHost {
    fn cite(&self, key: &str) {
        let mut cited = self.cited;
        if !key.is_empty() && !cited.peek().iter().any(|k| k == key) {
            cited.with_mut(|c| c.push(key.to_string()));
        }
    }

    async fn run(&self, call: ToolCall) -> Result<String, String> {
        let ws = self.ws;
        match call.name.as_str() {
            "workspace.list_sources" => {
                let mut out = String::from("id\tname\tfamily\tdialect\troot\n");
                for s in ws.sources.peek().iter() {
                    let d = &s.descriptor;
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
                let (source, handle) = source_of(ws, &call)?;
                let node = match call.str("node") {
                    Some(n) => parse_node(n)?,
                    None => handle.descriptor.root,
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
                if res.nodes.len() > limit {
                    let _ = writeln!(out, "… {} more", res.nodes.len() - limit);
                }
                Ok(out)
            }
            "graph.fetch" => {
                let (source, _) = source_of(ws, &call)?;
                let node = parse_node(call.str("node").ok_or("node is required")?)?;
                let (text, _) = source.fetch_text(node).await.map_err(|e| e.to_string())?;
                if let Ok(r) = source.query(Query::Node(node)).await {
                    if let Some(n) = r.nodes.first() {
                        self.cite(&n.native_key);
                    }
                }
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
            "source.text_query" => {
                let (source, _) = source_of(ws, &call)?;
                let res = source
                    .query(Query::Text {
                        dialect: call.str("dialect").unwrap_or("sql").to_string(),
                        text: call.str("text").unwrap_or_default().to_string(),
                    })
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(table_text(&res))
            }
            "index.search" => {
                let index = ws.index().ok_or("no folder is open (no index)")?;
                let query = call.str("query").unwrap_or_default().to_string();
                let res = index
                    .source
                    .query(Query::Text {
                        dialect: "search".into(),
                        text: query,
                    })
                    .await
                    .map_err(|e| e.to_string())?;
                for n in &res.nodes {
                    self.cite(&n.native_key);
                }
                Ok(table_text(&res))
            }
            "editor.open" => {
                let (source, _) = source_of(ws, &call)?;
                let node = parse_node(call.str("node").ok_or("node is required")?)?;
                let n = source
                    .query(Query::Node(node))
                    .await
                    .map_err(|e| e.to_string())?
                    .nodes
                    .into_iter()
                    .next()
                    .ok_or("no such node")?;
                self.cite(&n.native_key);
                let label = n.label.clone();
                match call.u64("line") {
                    Some(line) => ws
                        .reveal(n, line.saturating_sub(1) as u32, 0)
                        .await
                        .map_err(|e| e.to_string())?,
                    None => ws.open_node(n).await.map_err(|e| e.to_string())?,
                }
                Ok(format!("Opened {label}"))
            }
            other => Err(format!("unknown tool {other}")),
        }
    }
}

fn source_of(
    ws: Workspace,
    call: &ToolCall,
) -> Result<
    (
        std::sync::Arc<dyn moonkale_core::Source>,
        moonkale_ext_api::SourceHandle,
    ),
    String,
> {
    let id = call.str("source").ok_or("source is required")?;
    let sid = moonkale_core::SourceId::new(id);
    let handle = ws
        .sources
        .peek()
        .iter()
        .find(|s| s.descriptor.id == sid)
        .cloned()
        .ok_or_else(|| format!("unknown source {id}; call workspace.list_sources"))?;
    Ok((handle.source.clone(), handle))
}

fn parse_node(s: &str) -> Result<NodeId, String> {
    s.parse::<NodeId>()
        .map_err(|_| format!("{s} is not a node id"))
}

/// Rows as TSV (capped), then nodes when the query returned any.
fn table_text(res: &moonkale_core::QueryResult) -> String {
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
        if t.rows.len() > MAX_ROWS {
            let _ = writeln!(out, "… {} more rows", t.rows.len() - MAX_ROWS);
        }
    }
    if !res.nodes.is_empty() {
        let _ = writeln!(out, "{} nodes, {} edges:", res.nodes.len(), res.edges.len());
        for n in res.nodes.iter().take(MAX_NODES) {
            let _ = writeln!(out, "{}\t{:?}\t{}\t{}", n.id, n.kind, n.label, n.native_key);
        }
    }
    if out.is_empty() {
        out.push_str("(empty result)");
    }
    out
}

impl ToolHost for WorkspaceHost {
    fn call(&self, call: ToolCall) -> HostFuture<Result<String, String>> {
        let this = WorkspaceHost {
            ws: self.ws,
            pending: self.pending,
            cited: self.cited,
        };
        Box::pin(async move { this.run(call).await })
    }

    fn approve(&self, call: ToolCall, class: Class) -> HostFuture<bool> {
        let (tx, rx) = futures_channel::oneshot::channel();
        let mut pending = self.pending;
        pending.set(Some(PendingApproval {
            call,
            class,
            reply: std::rc::Rc::new(std::cell::RefCell::new(Some(tx))),
        }));
        Box::pin(async move {
            let answer = rx.await.unwrap_or(false);
            pending.set(None);
            answer
        })
    }
}
