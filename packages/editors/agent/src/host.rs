//! Tool execution against the workspace, and the approval hand-off to the UI.

use dioxus::prelude::*;
use moonkale_core::{Direction, NodeId, Query, SourceFamily};
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
    pub outer: String,
    pub owner: String,
    pub name: String,
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

    async fn offer_node(&self, node: &moonkale_core::Node, line: u64) -> String {
        let call = ToolCall {
            id: format!("display-{}", node.id),
            name: "editor.open".into(),
            input: serde_json::json!({"source": node.source.as_str(), "node": node.id.to_string(), "line": line}),
        };
        match crate::display::request(
            self.ws,
            self.outer.clone(),
            self.owner.clone(),
            self.name.clone(),
            call,
        )
        .await
        {
            Ok(note) => note,
            Err(error) => format!("View not opened: {error}"),
        }
    }

    async fn run(&self, call: ToolCall) -> Result<String, String> {
        let ws = self.ws;
        if *ws.saved_sessions.active.peek() != self.outer {
            return Err("conversation belongs to another session".into());
        }
        match call.name.as_str() {
            "ui.reveal" | "ui.select" | "ui.show_result" | "ui.canvas" | "editor.open" => {
                crate::display::request(
                    ws,
                    self.outer.clone(),
                    self.owner.clone(),
                    self.name.clone(),
                    call,
                )
                .await
            }
            "workspace.list_sources" => {
                let mut out = String::from("id\tname\tfamily\tdialect\troot\n");
                for s in ws.sources.open.peek().iter() {
                    let d = &s.descriptor;
                    // Every dialect, the main one first (`sql,query` for Sophia).
                    let names: Vec<_> = d.capabilities.text_dialects().map(|d| d.name()).collect();
                    let dialect = match (names.is_empty(), &d.family) {
                        (false, _) => names.join(","),
                        (true, SourceFamily::Index) => "search".into(),
                        (true, _) => "-".into(),
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
                let (text, version) = source.fetch_text(node).await.map_err(|e| e.to_string())?;
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
                    "[version {}] [chars {}..{} of {}]\n{}",
                    version.0,
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
            "editor.replace" | "editor.propose" => {
                crate::proposals::submit(ws, self.outer.clone(), self.name.clone(), call).await
            }
            "file.create" => {
                let (_, handle) = source_of(ws, &call)?;
                if handle.descriptor.family != SourceFamily::Folder {
                    return Err("file.create needs a folder source".into());
                }
                let path = call
                    .str("path")
                    .ok_or("path is required")?
                    .trim()
                    .to_string();
                let text = call.str("text").unwrap_or_default().to_string();
                let node = ws
                    .create_text(&handle.descriptor.id, handle.descriptor.root, &path, &text)
                    .await
                    .map_err(|e| e.to_string())?;
                self.cite(&node.native_key);
                let key = node.native_key.clone();
                let display = self.offer_node(&node, 1).await;
                Ok(format!(
                    "Created {key} ({} chars). {display}",
                    text.chars().count()
                ))
            }
            "terminal.run" => {
                let spawn_fn = ws
                    .spawn_terminal()
                    .ok_or("terminals are not available on this platform")?;
                let command = call
                    .str("command")
                    .ok_or("command is required")?
                    .to_string();
                let cwd = call.str("cwd").map(str::to_string).or_else(|| {
                    ws.sources
                        .open
                        .peek()
                        .iter()
                        .find(|s| s.descriptor.family == SourceFamily::Folder)
                        .and_then(|s| {
                            s.descriptor
                                .id
                                .as_str()
                                .strip_prefix("folder:")
                                .map(str::to_string)
                        })
                });
                let mut backend = spawn_fn(cwd, 120, 40).await.map_err(|e| e.to_string())?;
                let mut out = backend.take_output().ok_or("no output stream")?;
                // Run, then end the shell so the stream closes.
                backend.write(format!("{command}; exit $?\n").as_bytes());
                let mut relay = moonkale_terminal::Relay::new();
                let mut total = 0usize;
                use futures_util::StreamExt;
                // A command that never ends (`sleep infinity`, an open quote
                // swallowing the `exit`) must not wedge the turn (#13): a
                // wall-clock limit, then the shell is dropped.
                let mut deadline = std::pin::pin!(crate::sleep_ms(TERMINAL_RUN_MS));
                loop {
                    match futures_util::future::select(out.next(), deadline.as_mut()).await {
                        futures_util::future::Either::Left((Some(chunk), _)) => {
                            relay.set_private(backend.private_input());
                            relay.push(&chunk);
                            total += chunk.len();
                            if total > 200_000 {
                                break;
                            }
                        }
                        futures_util::future::Either::Left((None, _)) => break,
                        futures_util::future::Either::Right(_) => {
                            break;
                        }
                    }
                }
                relay.set_private(backend.private_input());
                let text = relay.preview();
                drop(backend);
                Ok(format!("$ {command}\n{text}"))
            }
            other => match wasm_owner(ws, other) {
                Some(ext) => ws.run_wasm_command(&ext, other, call.input.clone()).await,
                None => Err(format!("unknown tool {other}")),
            },
        }
    }
}

/// How long a tool call waits for the user's approval before it is declined.
const APPROVAL_MS: u32 = 600_000;

/// How long `terminal.run` waits for a command to finish.
const TERMINAL_RUN_MS: u32 = 120_000;

/// Commands of enabled wasm extensions as agent tools (`llm_tool` only).
pub fn wasm_tools(ws: Workspace) -> Vec<moonkale_llm::ToolDef> {
    let settings = ws.settings.resolved.peek();
    ws.contrib
        .wasm_extensions
        .peek()
        .iter()
        .filter(|m| settings.extensions.is_enabled_id(&m.id, false))
        .flat_map(|m| {
            m.commands
                .iter()
                .filter(|c| c.llm_tool)
                .map(move |c| moonkale_llm::ToolDef {
                    name: c.id.clone(),
                    description: format!("{} (extension {}: {})", c.description, m.name, c.title),
                    input_schema: c.input_schema.clone(),
                })
        })
        .collect()
}

/// Which wasm extension owns a command, if any.
fn wasm_owner(ws: Workspace, command: &str) -> Option<String> {
    let settings = ws.settings.resolved.peek();
    ws.contrib
        .wasm_extensions
        .peek()
        .iter()
        .find(|m| {
            settings.extensions.is_enabled_id(&m.id, false)
                && m.commands.iter().any(|c| c.id == command)
        })
        .map(|m| m.id.clone())
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
        .open
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
            outer: self.outer.clone(),
            owner: self.owner.clone(),
            name: self.name.clone(),
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
        // The card lives in the Agent panel; if that is closed or docked out
        // of sight, say so on the status line, and give up after a while
        // rather than keep the turn busy forever (#13).
        let mut ws = self.ws;
        ws.set_status(moonkale_ext_api::t!(ws, crate::L, "agent-approval-waiting"));
        Box::pin(async move {
            let answer = match futures_util::future::select(
                rx,
                std::pin::pin!(crate::sleep_ms(APPROVAL_MS)),
            )
            .await
            {
                futures_util::future::Either::Left((answer, _)) => answer.unwrap_or(false),
                futures_util::future::Either::Right(_) => {
                    ws.set_status(moonkale_ext_api::t!(ws, crate::L, "agent-approval-timeout"));
                    false
                }
            };
            pending.set(None);
            answer
        })
    }

    /// The target source classifies its own text queries.
    fn classify(&self, call: &ToolCall) -> Option<moonkale_core::Risk> {
        text_query_risk(call, |c| source_of(self.ws, c).ok().map(|(s, _)| s))
    }
}

/// `Source::classify` of the source a `source.text_query` targets; `None`
/// for other tools or an unknown source (the policy then uses the shared rules).
pub(crate) fn text_query_risk(
    call: &ToolCall,
    source: impl FnOnce(&ToolCall) -> Option<std::sync::Arc<dyn moonkale_core::Source>>,
) -> Option<moonkale_core::Risk> {
    if call.name != "source.text_query" {
        return None;
    }
    let s = source(call)?;
    Some(s.classify(
        call.str("dialect").unwrap_or("sql"),
        call.str("text").unwrap_or_default(),
    ))
}
