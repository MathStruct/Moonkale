//! `LspSession` — one connection to one language server.

use crate::transport::LspTransport;
use futures_channel::{mpsc, oneshot};
use futures_util::StreamExt;
use lsp_types as lsp;
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// 0-based line and UTF-16 column, as LSP defines them.
    pub line: u32,
    pub col: u32,
    pub end_line: u32,
    pub end_col: u32,
    /// `"error" | "warning" | "info" | "hint"`.
    pub severity: &'static str,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub uri: String,
    pub line: u32,
    pub col: u32,
}

/// A completion proposal, feature-neutral (Milestone 7).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CompletionItem {
    pub label: String,
    /// LSP `CompletionItemKind` as a lower-case word (`function`, `struct`…).
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// What to insert (defaults to the label). Snippet syntax is stripped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insert: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<String>,
}

/// One text edit in LSP coordinates (0-based line, UTF-16 column).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TextEdit {
    pub line: u32,
    pub col: u32,
    pub end_line: u32,
    pub end_col: u32,
    pub new_text: String,
}

/// Edits grouped per file URI (a `WorkspaceEdit` without the version dance).
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorkspaceEdit {
    pub changes: Vec<(String, Vec<TextEdit>)>,
}

impl WorkspaceEdit {
    /// Parse the LSP shape: `changes` and/or `documentChanges` (text
    /// document edits only; file create/rename/delete are ignored).
    pub fn from_lsp(v: &Value) -> Self {
        let mut out = WorkspaceEdit::default();
        let mut push = |uri: &str, edits: &Value| {
            let list: Vec<TextEdit> = edits
                .as_array()
                .map(|a| a.iter().filter_map(text_edit).collect())
                .unwrap_or_default();
            if list.is_empty() {
                return;
            }
            match out.changes.iter_mut().find(|(u, _)| u == uri) {
                Some((_, existing)) => existing.extend(list),
                None => out.changes.push((uri.to_string(), list)),
            }
        };
        if let Some(changes) = v.get("changes").and_then(Value::as_object) {
            for (uri, edits) in changes {
                push(uri, edits);
            }
        }
        if let Some(docs) = v.get("documentChanges").and_then(Value::as_array) {
            for d in docs {
                if let (Some(uri), Some(edits)) = (
                    d.pointer("/textDocument/uri").and_then(Value::as_str),
                    d.get("edits"),
                ) {
                    push(uri, edits);
                }
            }
        }
        out
    }

    /// Apply one file's edits to `text` (edits are applied last-to-first so
    /// earlier positions stay valid). Overlapping edits are applied in the
    /// order given.
    pub fn apply_to_text(text: &str, edits: &[TextEdit]) -> String {
        let mut sorted: Vec<&TextEdit> = edits.iter().collect();
        sorted.sort_by_key(|e| std::cmp::Reverse((e.line, e.col)));
        let mut chars: Vec<char> = text.chars().collect();
        for e in sorted {
            let start = char_offset(&chars, e.line, e.col);
            let end = char_offset(&chars, e.end_line, e.end_col).max(start);
            chars.splice(start..end, e.new_text.chars());
        }
        chars.into_iter().collect()
    }
}

/// Char index of an LSP position (line, UTF-16 column) in `chars`; clamps
/// to the line end and to the document end.
pub fn char_offset(chars: &[char], line: u32, col: u32) -> usize {
    let mut i = 0usize;
    let mut l = 0u32;
    while l < line && i < chars.len() {
        if chars[i] == '\n' {
            l += 1;
        }
        i += 1;
    }
    let mut units = 0u32;
    while i < chars.len() && chars[i] != '\n' && units < col {
        units += chars[i].len_utf16() as u32;
        i += 1;
    }
    i
}

fn text_edit(v: &Value) -> Option<TextEdit> {
    let r = v.get("range")?;
    Some(TextEdit {
        line: r.pointer("/start/line")?.as_u64()? as u32,
        col: r.pointer("/start/character")?.as_u64()? as u32,
        end_line: r.pointer("/end/line")?.as_u64()? as u32,
        end_col: r.pointer("/end/character")?.as_u64()? as u32,
        new_text: v.get("newText")?.as_str()?.to_string(),
    })
}

/// A code action as offered to the user: the title, and either a ready
/// edit or the raw action to resolve.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CodeAction {
    pub title: String,
    pub kind: String,
    pub edit: Option<WorkspaceEdit>,
    /// The action as the server sent it (for `codeAction/resolve`).
    pub raw: Value,
}

/// Things the session reports to whoever owns it (the editor extension).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LspEvent {
    Initialized {
        server: String,
    },
    Diagnostics {
        uri: String,
        diagnostics: Vec<Diagnostic>,
    },
    /// `$/progress` / `window/showMessage`, for the status bar.
    Status(String),
    Closed,
}

struct Inner {
    transport: Box<dyn LspTransport>,
    next_id: i64,
    pending: HashMap<i64, oneshot::Sender<Result<Value, String>>>,
    initialized: bool,
}

#[derive(Clone)]
pub struct LspSession {
    inner: Rc<RefCell<Inner>>,
    events: mpsc::UnboundedSender<LspEvent>,
}

impl LspSession {
    /// Wrap a transport; incoming messages are dispatched by [`pump`] which
    /// the caller must drive (spawn it).
    ///
    /// [`pump`]: LspSession::pump
    pub fn new(transport: Box<dyn LspTransport>) -> (Self, mpsc::UnboundedReceiver<LspEvent>) {
        let (tx, rx) = mpsc::unbounded();
        let s = Self {
            inner: Rc::new(RefCell::new(Inner {
                transport,
                next_id: 1,
                pending: HashMap::new(),
                initialized: false,
            })),
            events: tx,
        };
        (s, rx)
    }

    /// Read incoming messages until the transport closes. Responses resolve
    /// pending requests; notifications become [`LspEvent`]s; server→client
    /// requests get the minimal reply that keeps servers happy.
    pub async fn pump(self) {
        let Some(mut incoming) = self.inner.borrow_mut().transport.take_incoming() else {
            return;
        };
        while let Some(text) = incoming.next().await {
            let Ok(msg) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            self.handle(msg);
        }
        let _ = self.events.unbounded_send(LspEvent::Closed);
    }

    fn handle(&self, msg: Value) {
        let id = msg.get("id").cloned();
        let method = msg
            .get("method")
            .and_then(Value::as_str)
            .map(str::to_string);
        match (id, method) {
            // Response to one of ours.
            (Some(id), None) => {
                if let Some(id) = id.as_i64() {
                    if let Some(tx) = self.inner.borrow_mut().pending.remove(&id) {
                        let result = match msg.get("error") {
                            Some(e) => Err(e
                                .get("message")
                                .and_then(Value::as_str)
                                .unwrap_or("error")
                                .to_string()),
                            None => Ok(msg.get("result").cloned().unwrap_or(Value::Null)),
                        };
                        let _ = tx.send(result);
                    }
                }
            }
            // Server → client request: answer what we can, refuse the rest politely.
            (Some(id), Some(method)) => {
                let result = match method.as_str() {
                    "window/workDoneProgress/create"
                    | "client/registerCapability"
                    | "client/unregisterCapability" => json!(null),
                    "workspace/configuration" => {
                        let n = msg
                            .pointer("/params/items")
                            .and_then(Value::as_array)
                            .map(|a| a.len())
                            .unwrap_or(1);
                        json!(vec![Value::Null; n])
                    }
                    _ => {
                        self.send(json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": "unsupported" } }));
                        return;
                    }
                };
                self.send(json!({ "jsonrpc": "2.0", "id": id, "result": result }));
            }
            // Notification.
            (None, Some(method)) => match method.as_str() {
                "textDocument/publishDiagnostics" => {
                    if let Ok(p) = serde_json::from_value::<lsp::PublishDiagnosticsParams>(
                        msg.get("params").cloned().unwrap_or(Value::Null),
                    ) {
                        let diagnostics = p.diagnostics.into_iter().map(convert_diag).collect();
                        let _ = self.events.unbounded_send(LspEvent::Diagnostics {
                            uri: p.uri.to_string(),
                            diagnostics,
                        });
                    }
                }
                "$/progress" => {
                    let title = msg.pointer("/params/value/title").and_then(Value::as_str);
                    let message = msg.pointer("/params/value/message").and_then(Value::as_str);
                    let kind = msg
                        .pointer("/params/value/kind")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    // Progress messages can be long paths; keep the status bar short.
                    let short = |m: &str| m.split(": ").next().unwrap_or(m).to_string();
                    let text = match (kind, title, message) {
                        ("end", _, _) => "ready".to_string(),
                        (_, Some(t), Some(m)) => format!("{t}: {}", short(m)),
                        (_, Some(t), None) => t.to_string(),
                        (_, None, Some(m)) => short(m),
                        _ => return,
                    };
                    let _ = self.events.unbounded_send(LspEvent::Status(text));
                }
                "window/showMessage" | "window/logMessage" if method == "window/showMessage" => {
                    if let Some(m) = msg.pointer("/params/message").and_then(Value::as_str) {
                        let _ = self.events.unbounded_send(LspEvent::Status(m.to_string()));
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn send(&self, msg: Value) {
        self.inner.borrow().transport.send(msg.to_string());
    }

    fn notify(&self, method: &str, params: Value) {
        self.send(json!({ "jsonrpc": "2.0", "method": method, "params": params }));
    }

    async fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        let (tx, rx) = oneshot::channel();
        let id = {
            let mut inner = self.inner.borrow_mut();
            let id = inner.next_id;
            inner.next_id += 1;
            inner.pending.insert(id, tx);
            id
        };
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        rx.await.unwrap_or_else(|_| Err("session closed".into()))
    }

    /// The handshake. `root` is an absolute path.
    pub async fn initialize(&self, root: &str) -> Result<String, String> {
        let root_uri = format!("file://{root}");
        let params = json!({
            "processId": null,
            "rootUri": root_uri,
            "workspaceFolders": [{ "uri": root_uri, "name": "workspace" }],
            "capabilities": {
                "textDocument": {
                    "synchronization": { "didSave": true },
                    "publishDiagnostics": { "relatedInformation": false },
                    "hover": { "contentFormat": ["markdown", "plaintext"] },
                    "definition": {},
                    "completion": { "completionItem": { "snippetSupport": false, "insertReplaceSupport": false }, "contextSupport": false },
                    "rename": { "prepareSupport": false },
                    "codeAction": { "codeActionLiteralSupport": { "codeActionKind": { "valueSet": ["quickfix", "refactor", "refactor.extract", "refactor.inline", "refactor.rewrite", "source"] } }, "resolveSupport": { "properties": ["edit"] }, "dataSupport": true },
                    "references": {}
                },
                "window": { "workDoneProgress": true },
                "workspace": { "configuration": true, "workspaceFolders": true }
            },
            "clientInfo": { "name": "moonkale", "version": "0.1" }
        });
        let result = self.request("initialize", params).await?;
        let server = result
            .pointer("/serverInfo/name")
            .and_then(Value::as_str)
            .unwrap_or("language server")
            .to_string();
        self.notify("initialized", json!({}));
        self.inner.borrow_mut().initialized = true;
        let _ = self.events.unbounded_send(LspEvent::Initialized {
            server: server.clone(),
        });
        Ok(server)
    }

    pub fn is_initialized(&self) -> bool {
        self.inner.borrow().initialized
    }

    pub fn did_open(&self, uri: &str, language: &str, version: i32, text: &str) {
        self.notify("textDocument/didOpen", json!({ "textDocument": { "uri": uri, "languageId": language, "version": version, "text": text } }));
    }

    /// Full-document sync (`TextDocumentSyncKind::Full`).
    pub fn did_change(&self, uri: &str, version: i32, text: &str) {
        self.notify("textDocument/didChange", json!({ "textDocument": { "uri": uri, "version": version }, "contentChanges": [{ "text": text }] }));
    }

    pub fn did_save(&self, uri: &str) {
        self.notify(
            "textDocument/didSave",
            json!({ "textDocument": { "uri": uri } }),
        );
    }

    pub fn did_close(&self, uri: &str) {
        self.notify(
            "textDocument/didClose",
            json!({ "textDocument": { "uri": uri } }),
        );
    }

    /// Hover text (markdown) at a position, if any.
    pub async fn hover(&self, uri: &str, line: u32, col: u32) -> Result<Option<String>, String> {
        let r = self.request("textDocument/hover", json!({ "textDocument": { "uri": uri }, "position": { "line": line, "character": col } })).await?;
        if r.is_null() {
            return Ok(None);
        }
        let hover: lsp::Hover = serde_json::from_value(r).map_err(|e| e.to_string())?;
        Ok(Some(hover_text(hover.contents)))
    }

    /// First definition location, if any.
    pub async fn definition(
        &self,
        uri: &str,
        line: u32,
        col: u32,
    ) -> Result<Option<Location>, String> {
        let r = self.request("textDocument/definition", json!({ "textDocument": { "uri": uri }, "position": { "line": line, "character": col } })).await?;
        Ok(first_location(r))
    }
}

impl LspSession {
    /// Completion proposals at a position (Milestone 7). Snippet
    /// placeholders (`$0`, `${1:x}`) are stripped from insert texts.
    pub async fn completion(
        &self,
        uri: &str,
        line: u32,
        col: u32,
    ) -> Result<Vec<CompletionItem>, String> {
        let r = self
            .request(
                "textDocument/completion",
                json!({ "textDocument": { "uri": uri }, "position": { "line": line, "character": col } }),
            )
            .await?;
        let items = match &r {
            Value::Array(a) => a.clone(),
            Value::Object(o) => o
                .get("items")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        Ok(items
            .iter()
            .filter_map(|it| {
                let label = it.get("label")?.as_str()?.to_string();
                let kind = it
                    .get("kind")
                    .and_then(Value::as_u64)
                    .map(kind_name)
                    .unwrap_or("text")
                    .to_string();
                let insert = it
                    .pointer("/textEdit/newText")
                    .or_else(|| it.get("insertText"))
                    .and_then(Value::as_str)
                    .map(strip_snippet)
                    .filter(|t| t != &label);
                Some(CompletionItem {
                    label,
                    kind,
                    detail: it.get("detail").and_then(Value::as_str).map(str::to_string),
                    insert,
                    sort: it
                        .get("sortText")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                })
            })
            .take(200)
            .collect())
    }

    /// Rename the symbol at a position everywhere the server knows about.
    pub async fn rename(
        &self,
        uri: &str,
        line: u32,
        col: u32,
        new_name: &str,
    ) -> Result<WorkspaceEdit, String> {
        let r = self
            .request(
                "textDocument/rename",
                json!({ "textDocument": { "uri": uri }, "position": { "line": line, "character": col }, "newName": new_name }),
            )
            .await?;
        Ok(WorkspaceEdit::from_lsp(&r))
    }

    /// Code actions for a range (quick fixes and refactors with an edit).
    pub async fn code_actions(
        &self,
        uri: &str,
        line: u32,
        col: u32,
        end_line: u32,
        end_col: u32,
    ) -> Result<Vec<CodeAction>, String> {
        let r = self
            .request(
                "textDocument/codeAction",
                json!({
                    "textDocument": { "uri": uri },
                    "range": { "start": { "line": line, "character": col }, "end": { "line": end_line, "character": end_col } },
                    "context": { "diagnostics": [] }
                }),
            )
            .await?;
        Ok(r.as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| {
                        let title = v.get("title")?.as_str()?.to_string();
                        // Bare `Command`s (no edit, no data) cannot be applied here.
                        if v.get("edit").is_none() && v.get("data").is_none() {
                            return None;
                        }
                        Some(CodeAction {
                            title,
                            kind: v
                                .get("kind")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string(),
                            edit: v.get("edit").map(WorkspaceEdit::from_lsp),
                            raw: v.clone(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    /// The edit of a code action, resolving it with the server when it was
    /// sent without one.
    pub async fn resolve_code_action(&self, action: &CodeAction) -> Result<WorkspaceEdit, String> {
        if let Some(e) = &action.edit {
            return Ok(e.clone());
        }
        let r = self
            .request("codeAction/resolve", action.raw.clone())
            .await?;
        match r.get("edit") {
            Some(e) => Ok(WorkspaceEdit::from_lsp(e)),
            None => Err("the server returned no edit".into()),
        }
    }

    /// Every reference to the symbol at a position (including the declaration).
    pub async fn references(
        &self,
        uri: &str,
        line: u32,
        col: u32,
    ) -> Result<Vec<Location>, String> {
        let r = self
            .request(
                "textDocument/references",
                json!({ "textDocument": { "uri": uri }, "position": { "line": line, "character": col }, "context": { "includeDeclaration": true } }),
            )
            .await?;
        Ok(r.as_array()
            .map(|a| a.iter().filter_map(|l| first_location(l.clone())).collect())
            .unwrap_or_default())
    }
}

fn kind_name(k: u64) -> &'static str {
    match k {
        1 => "text",
        2 => "method",
        3 => "function",
        4 => "constructor",
        5 => "field",
        6 => "variable",
        7 => "class",
        8 => "interface",
        9 => "module",
        10 => "property",
        11 => "unit",
        12 => "value",
        13 => "enum",
        14 => "keyword",
        15 => "snippet",
        16 => "color",
        17 => "file",
        18 => "reference",
        19 => "folder",
        20 => "enumMember",
        21 => "constant",
        22 => "struct",
        23 => "event",
        24 => "operator",
        25 => "typeParameter",
        _ => "text",
    }
}

/// `foo($0)` → `foo()`, `${1:name}` → `name`, `\$` → `$`.
pub fn strip_snippet(t: &str) -> String {
    let mut out = String::new();
    let mut chars = t.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(n) = chars.next() {
                    out.push(n);
                }
            }
            '$' => match chars.peek() {
                Some('{') => {
                    chars.next();
                    let mut inner = String::new();
                    let mut depth = 1;
                    for n in chars.by_ref() {
                        if n == '{' {
                            depth += 1;
                        } else if n == '}' {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        inner.push(n);
                    }
                    // `${1:text}` keeps the text; `${1}` keeps nothing.
                    if let Some((_, text)) = inner.split_once(':') {
                        out.push_str(&strip_snippet(text));
                    }
                }
                Some(d) if d.is_ascii_digit() => {
                    while matches!(chars.peek(), Some(d) if d.is_ascii_digit()) {
                        chars.next();
                    }
                }
                _ => out.push('$'),
            },
            _ => out.push(c),
        }
    }
    out
}

fn convert_diag(d: lsp::Diagnostic) -> Diagnostic {
    Diagnostic {
        line: d.range.start.line,
        col: d.range.start.character,
        end_line: d.range.end.line,
        end_col: d.range.end.character,
        severity: match d.severity {
            Some(lsp::DiagnosticSeverity::ERROR) => "error",
            Some(lsp::DiagnosticSeverity::WARNING) => "warning",
            Some(lsp::DiagnosticSeverity::HINT) => "hint",
            _ => "info",
        },
        message: d.message,
    }
}

/// Hover contents as plain text: the tooltip is a `<div>`, so markdown code
/// fences are stripped and blank lines collapsed. Rendering markdown is M4.
fn hover_text(contents: lsp::HoverContents) -> String {
    use lsp::{HoverContents, MarkedString};
    let marked = |m: MarkedString| match m {
        MarkedString::String(s) => s,
        MarkedString::LanguageString(l) => l.value,
    };
    let raw = match contents {
        HoverContents::Scalar(m) => marked(m),
        HoverContents::Array(v) => v.into_iter().map(marked).collect::<Vec<_>>().join("\n\n"),
        HoverContents::Markup(m) => m.value,
    };
    let mut out = String::new();
    let mut blank = true;
    for line in raw.lines() {
        if line.trim_start().starts_with("```") || line.trim() == "---" {
            continue;
        }
        if line.trim().is_empty() {
            if !blank {
                out.push('\n');
            }
            blank = true;
            continue;
        }
        out.push_str(line);
        out.push('\n');
        blank = false;
    }
    out.trim_end().to_string()
}

fn first_location(v: Value) -> Option<Location> {
    let one = |l: &Value| {
        let uri = l
            .get("uri")
            .or_else(|| l.get("targetUri"))?
            .as_str()?
            .to_string();
        let range = l
            .get("range")
            .or_else(|| l.get("targetSelectionRange"))
            .or_else(|| l.get("targetRange"))?;
        Some(Location {
            uri,
            line: range.pointer("/start/line")?.as_u64()? as u32,
            col: range.pointer("/start/character")?.as_u64()? as u32,
        })
    };
    match &v {
        Value::Array(a) => a.first().and_then(one),
        Value::Object(_) => one(&v),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A transport that records what the client sends and lets the test
    /// inject server messages.
    struct Fake {
        sent: Rc<RefCell<Vec<Value>>>,
        incoming: Option<mpsc::UnboundedReceiver<String>>,
    }
    impl LspTransport for Fake {
        fn send(&self, message: String) {
            self.sent
                .borrow_mut()
                .push(serde_json::from_str(&message).unwrap());
        }
        fn take_incoming(&mut self) -> Option<mpsc::UnboundedReceiver<String>> {
            self.incoming.take()
        }
    }

    #[tokio::test]
    async fn initialize_handshake_and_diagnostics() {
        let sent = Rc::new(RefCell::new(Vec::new()));
        let (server_tx, server_rx) = mpsc::unbounded::<String>();
        let (session, mut events) = LspSession::new(Box::new(Fake {
            sent: sent.clone(),
            incoming: Some(server_rx),
        }));
        let local = tokio::task::LocalSet::new();
        local.spawn_local(session.clone().pump());
        local
            .run_until(async move {
                let s2 = session.clone();
                let init = tokio::task::spawn_local(async move { s2.initialize("/tmp/proj").await });
                tokio::task::yield_now().await;
                // The client sent `initialize` with id 1; answer it.
                let first = sent.borrow()[0].clone();
                assert_eq!(first["method"], "initialize");
                assert_eq!(first["params"]["rootUri"], "file:///tmp/proj");
                server_tx.unbounded_send(json!({ "jsonrpc": "2.0", "id": 1, "result": { "capabilities": {}, "serverInfo": { "name": "fake-ls" } } }).to_string()).unwrap();
                assert_eq!(init.await.unwrap().unwrap(), "fake-ls");
                assert_eq!(sent.borrow()[1]["method"], "initialized");
                assert!(matches!(events.next().await, Some(LspEvent::Initialized { .. })));

                // A diagnostics notification becomes an event.
                server_tx
                    .unbounded_send(json!({ "jsonrpc": "2.0", "method": "textDocument/publishDiagnostics", "params": { "uri": "file:///tmp/proj/src/main.rs", "diagnostics": [{ "range": { "start": { "line": 2, "character": 4 }, "end": { "line": 2, "character": 9 } }, "severity": 1, "message": "boom" }] } }).to_string())
                    .unwrap();
                match events.next().await {
                    Some(LspEvent::Diagnostics { uri, diagnostics }) => {
                        assert!(uri.ends_with("main.rs"));
                        assert_eq!(diagnostics[0], Diagnostic { line: 2, col: 4, end_line: 2, end_col: 9, severity: "error", message: "boom".into() });
                    }
                    other => panic!("{other:?}"),
                }
                // Server → client request gets an answer.
                server_tx.unbounded_send(json!({ "jsonrpc": "2.0", "id": 77, "method": "workspace/configuration", "params": { "items": [{}, {}] } }).to_string()).unwrap();
                tokio::task::yield_now().await;
                let reply = sent.borrow().iter().find(|m| m["id"] == 77).cloned().expect("reply");
                assert_eq!(reply["result"], json!([null, null]));
                // Hover round trip.
                let s3 = session.clone();
                let h = tokio::task::spawn_local(async move { s3.hover("file:///tmp/proj/src/main.rs", 0, 0).await });
                tokio::task::yield_now().await;
                let req = sent.borrow().iter().find(|m| m["method"] == "textDocument/hover").cloned().unwrap();
                server_tx.unbounded_send(json!({ "jsonrpc": "2.0", "id": req["id"], "result": { "contents": { "kind": "markdown", "value": "fn main()" } } }).to_string()).unwrap();
                assert_eq!(h.await.unwrap().unwrap(), Some("fn main()".to_string()));
            })
            .await;
    }
}

#[cfg(test)]
mod m7_tests {
    use super::*;

    #[test]
    fn snippets_are_stripped() {
        assert_eq!(strip_snippet("foo($0)"), "foo()");
        assert_eq!(strip_snippet("${1:name}.len()"), "name.len()");
        assert_eq!(strip_snippet("a\\$b ${2}"), "a$b ");
    }

    #[test]
    fn workspace_edit_parses_both_shapes_and_applies_in_order() {
        let v = json!({
            "changes": { "file:///a.rs": [
                { "range": { "start": { "line": 0, "character": 3 }, "end": { "line": 0, "character": 6 } }, "newText": "bar" },
                { "range": { "start": { "line": 1, "character": 0 }, "end": { "line": 1, "character": 3 } }, "newText": "bar" }
            ] },
            "documentChanges": [ { "textDocument": { "uri": "file:///b.rs", "version": 3 }, "edits": [
                { "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } }, "newText": "// " }
            ] } ]
        });
        let e = WorkspaceEdit::from_lsp(&v);
        assert_eq!(e.changes.len(), 2);
        let a = &e
            .changes
            .iter()
            .find(|(u, _)| u == "file:///a.rs")
            .unwrap()
            .1;
        assert_eq!(
            WorkspaceEdit::apply_to_text("fn foo() {}\nfoo();\n", a),
            "fn bar() {}\nbar();\n"
        );
        // UTF-16 columns: an emoji is two units.
        assert_eq!(char_offset(&"a😀b".chars().collect::<Vec<_>>(), 0, 3), 2);
        assert_eq!(char_offset(&"x\ny".chars().collect::<Vec<_>>(), 1, 1), 3);
    }
}
