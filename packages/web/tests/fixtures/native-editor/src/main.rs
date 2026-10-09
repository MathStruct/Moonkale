//! Web and desktop fixture exercising the production Rust panel with real Workspace state.
use dioxus::prelude::*;
use moonkale_code_view::RustCodeEditorPanel;
use moonkale_core::*;
use moonkale_ext_api::{
    document::Document, AttachFuture, Command, EditorAction, FolderAccess, OpenFolderFuture,
    OpenOptions, Workspace, WorkspaceConfig,
};
use std::sync::{Arc, Mutex};
thread_local! {
    static WIKI_CREATED: std::cell::RefCell<std::collections::HashMap<NodeId, (Node, String)>> = std::cell::RefCell::new(std::collections::HashMap::new());
    static WIKI_INDEX: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
    static WIKI_DELAY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static LSP_LOG: std::cell::RefCell<Option<Signal<Vec<String>>>> = const { std::cell::RefCell::new(None) };
    static COMPLETION_MODE: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
    static COMPLETION_DELAYED: std::cell::RefCell<Vec<(futures_channel::mpsc::UnboundedSender<String>, serde_json::Value)>> = const { std::cell::RefCell::new(Vec::new()) };
    static ACTIONS_MODE: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
    static REFERENCES_MODE: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
    static RESOLVE_MODE: std::cell::Cell<u8> = const { std::cell::Cell::new(1) };
    static TOOLS_DELAYED: std::cell::RefCell<Vec<(futures_channel::mpsc::UnboundedSender<String>, serde_json::Value)>> = const { std::cell::RefCell::new(Vec::new()) };
    static RENAME_MODE: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
    static RENAME_DELAYED: std::cell::RefCell<Vec<(futures_channel::mpsc::UnboundedSender<String>, serde_json::Value)>> = const { std::cell::RefCell::new(Vec::new()) };
    static DEFINITION_MODE: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
    static DEFINITION_DELAYED: std::cell::RefCell<Vec<(futures_channel::mpsc::UnboundedSender<String>, serde_json::Value)>> = const { std::cell::RefCell::new(Vec::new()) };
    static DEFINITION_LOAD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static HOVER_MODE: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
    static HOVER_DELAYED: std::cell::RefCell<Vec<(futures_channel::mpsc::UnboundedSender<String>, serde_json::Value)>> = const { std::cell::RefCell::new(Vec::new()) };
    static LSP_SEND: std::cell::RefCell<Option<futures_channel::mpsc::UnboundedSender<String>>> = const { std::cell::RefCell::new(None) };
}
struct FakeLsp {
    sender: futures_channel::mpsc::UnboundedSender<String>,
    incoming: Option<futures_channel::mpsc::UnboundedReceiver<String>>,
}
impl moonkale_lsp::LspTransport for FakeLsp {
    fn send(&self, message: String) {
        let message: serde_json::Value = serde_json::from_str(&message).unwrap();
        let mut summary = message.clone();
        for pointer in ["/params/textDocument/text", "/params/contentChanges/0/text"] {
            if let Some(value) = summary.pointer_mut(pointer) {
                if let Some(text) = value.as_str() {
                    if text.len() > 4096 {
                        *value = serde_json::json!(format!("[{} bytes]", text.len()));
                    }
                }
            }
        }
        LSP_LOG.with(|log| {
            if let Some(mut value) = *log.borrow() {
                value.write().push(summary.to_string());
            }
        });
        if message["method"] == "textDocument/completion" {
            let mode = COMPLETION_MODE.with(|mode| mode.get());
            if mode == 2 {
                COMPLETION_DELAYED.with(|pending| {
                    pending
                        .borrow_mut()
                        .push((self.sender.clone(), message.clone()))
                });
            } else {
                respond_completion(&self.sender, &message, mode);
            }
        }
        if matches!(
            message["method"].as_str(),
            Some("textDocument/codeAction" | "codeAction/resolve" | "textDocument/references")
        ) {
            let mode = match message["method"].as_str().unwrap() {
                "textDocument/codeAction" => ACTIONS_MODE.with(|mode| mode.get()),
                "codeAction/resolve" => RESOLVE_MODE.with(|mode| mode.get()),
                _ => REFERENCES_MODE.with(|mode| mode.get()),
            };
            if mode == 2 {
                TOOLS_DELAYED.with(|pending| {
                    pending
                        .borrow_mut()
                        .push((self.sender.clone(), message.clone()))
                });
            } else {
                respond_tools(&self.sender, &message, mode);
            }
        }
        if message["method"] == "textDocument/rename" {
            let mode = RENAME_MODE.with(|mode| mode.get());
            if mode == 2 {
                RENAME_DELAYED.with(|pending| {
                    pending
                        .borrow_mut()
                        .push((self.sender.clone(), message.clone()))
                });
            } else {
                respond_rename(&self.sender, &message, mode);
            }
        }
        if message["method"] == "textDocument/definition" {
            let mode = DEFINITION_MODE.with(|mode| mode.get());
            if mode == 3 {
                DEFINITION_DELAYED.with(|pending| {
                    pending
                        .borrow_mut()
                        .push((self.sender.clone(), message.clone()))
                });
            } else {
                respond_definition(&self.sender, &message, mode);
            }
        }
        if message["method"] == "textDocument/hover" {
            match HOVER_MODE.with(|mode| mode.get()) {
                1 => HOVER_DELAYED.with(|pending| {
                    pending
                        .borrow_mut()
                        .push((self.sender.clone(), message.clone()))
                }),
                2 => {
                    self.sender
                        .unbounded_send(
                            serde_json::json!({"jsonrpc":"2.0", "id":message["id"], "result":null})
                                .to_string(),
                        )
                        .unwrap();
                }
                3 => {
                    self.sender.unbounded_send(serde_json::json!({"jsonrpc":"2.0", "id":message["id"], "error":{"code":-32603,"message":"fixture hover error"}}).to_string()).unwrap();
                }
                4 => {
                    self.sender.unbounded_send(serde_json::json!({"jsonrpc":"2.0", "id":message["id"], "result":{"contents":"  "}}).to_string()).unwrap();
                }
                _ => respond_hover(&self.sender, &message),
            }
        }
        if message["method"] == "initialize" {
            self.sender.unbounded_send(serde_json::json!({"jsonrpc":"2.0", "id":message["id"], "result":{"capabilities":{}, "serverInfo":{"name":"fixture-lsp"}}}).to_string()).unwrap();
        }
    }
    fn take_incoming(&mut self) -> Option<futures_channel::mpsc::UnboundedReceiver<String>> {
        self.incoming.take()
    }
}
fn respond_completion(
    sender: &futures_channel::mpsc::UnboundedSender<String>,
    request: &serde_json::Value,
    mode: u8,
) {
    let line = request["params"]["position"]["line"].as_u64().unwrap();
    let col = request["params"]["position"]["character"].as_u64().unwrap();
    let response = if mode == 3 {
        serde_json::json!({"jsonrpc":"2.0","id":request["id"],"error":{"code":-32603,"message":"completion error"}})
    } else {
        let items = match mode {
            1 => serde_json::json!([
                {"label":"printBeta", "kind":3, "sortText":"2", "detail":"second"},
                {"label":"printAlpha", "kind":3, "sortText":"1", "detail":"<safe> first"}
            ]),
            5 => serde_json::json!([{"label":"printStale", "kind":3}]),
            6 => serde_json::Value::Array(
                (0..30)
                    .map(|index| serde_json::json!({"label":format!("print{index:02}"), "kind":3}))
                    .collect(),
            ),
            4 => {
                serde_json::json!([{"label":"print", "filterText":"pri", "kind":3, "insertTextFormat":2,
                "textEdit":{"range":{"start":{"line":line,"character":col.saturating_sub(3)},"end":{"line":line,"character":col}},"newText":"print(${1:value})$0"},
                "additionalTextEdits":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":0}},"newText":"use thing;\n"}]}])
            }
            _ => serde_json::json!([]),
        };
        serde_json::json!({"jsonrpc":"2.0","id":request["id"],"result":{"isIncomplete":false,"items":items}})
    };
    sender.unbounded_send(response.to_string()).unwrap();
}
fn respond_tools(
    sender: &futures_channel::mpsc::UnboundedSender<String>,
    request: &serde_json::Value,
    mode: u8,
) {
    let method = request["method"].as_str().unwrap();
    let origin = "file:///tmp/native-fixture/main.rs";
    let target = "file:///tmp/native-fixture/defs/target%20%23%C3%A9.rs";
    let text_edit = |line: u32, col: u32, end: u32, text: &str| serde_json::json!({"range":{"start":{"line":line,"character":col},"end":{"line":line,"character":end}},"newText":text});
    let edit = |text: &str| serde_json::json!({"changes":{origin:[text_edit(0,2,5,text),text_edit(0,6,9,text)]}});
    let result = if method == "textDocument/codeAction" {
        match mode {
            1 => serde_json::json!([
                {"title":"Unavailable","disabled":{"reason":"Cannot fix this"}},
                {"title":"Direct <safe> fix","kind":"quickfix","edit":edit("fixed")},
                {"title":"Preferred resolved fix","kind":"refactor","isPreferred":true,"data":{"token":42}},
                {"title":"Command-only","command":"run"},
                {"title":"Partial command","edit":edit("bad"),"command":{"command":"run"}}
            ]),
            5|6 => serde_json::json!([{"title":"Fix two files","kind":"quickfix","edit":{"documentChanges":[{"textDocument":{"uri":origin,"version":null},"edits":[text_edit(0,2,5,"fixed"),text_edit(0,6,9,"fixed")]},{"textDocument":{"uri":target,"version":null},"edits":[text_edit(1,2,if mode==6 {99} else {7},"fixed")]}]}}]),
            7 => serde_json::json!([{"title":"Broken","edit":{"changes":{origin:[text_edit(0,2,5,"bad"),{"newText":"bad"}]}}}]),
            8 => serde_json::json!((0..30).map(|index| serde_json::json!({"title":format!("Choice {index}"),"edit":edit(&format!("fixed{index}"))})).collect::<Vec<_>>()),
            _ => serde_json::Value::Null,
        }
    } else if method == "codeAction/resolve" {
        match mode {
            1 => serde_json::json!({"title":"Preferred resolved fix","edit":edit("resolved")}),
            4 => {
                serde_json::json!({"title":"Broken resolve","edit":{"changes":{origin:[{"newText":"bad"}]}}})
            }
            5 => {
                serde_json::json!({"title":"Command resolve","edit":edit("bad"),"command":{"command":"run"}})
            }
            _ => serde_json::Value::Null,
        }
    } else {
        let location = |uri: &str, line: u32, col: u32| serde_json::json!({"uri":uri,"range":{"start":{"line":line,"character":col},"end":{"line":line,"character":col+1}}});
        match mode {
            1 => {
                serde_json::json!([location(origin,0,2),location(target,1,2),location(origin,0,2),location("file:///tmp/native-fixture-other/a.rs",0,0),{"uri":origin,"range":{}}])
            }
            5 => serde_json::json!([location(target, 1, 2)]),
            6 => serde_json::json!([location(origin, 1, 9)]),
            7 => serde_json::json!([location("file:///tmp/native-fixture/missing.rs", 0, 0)]),
            8 => serde_json::json!([location("file:///tmp/native-fixture/%ZZ.rs", 0, 0)]),
            _ => serde_json::Value::Null,
        }
    };
    let error = (method == "textDocument/codeAction" && mode == 4)
        || (method == "textDocument/references" && mode == 4)
        || (method == "codeAction/resolve" && mode == 3);
    let response = if error {
        serde_json::json!({"jsonrpc":"2.0","id":request["id"],"error":{"code":-32603,"message":"tools error"}})
    } else {
        serde_json::json!({"jsonrpc":"2.0","id":request["id"],"result":result})
    };
    sender.unbounded_send(response.to_string()).unwrap();
}

fn respond_rename(
    sender: &futures_channel::mpsc::UnboundedSender<String>,
    request: &serde_json::Value,
    mode: u8,
) {
    let uri = request["params"]["textDocument"]["uri"].as_str().unwrap();
    let name = request["params"]["newName"].as_str().unwrap();
    let edit = |line: u32, col: u32, end: u32| serde_json::json!({"range":{"start":{"line":line,"character":col},"end":{"line":line,"character":end}},"newText":name});
    let edits = vec![edit(0, 2, 5), edit(0, 6, 9)];
    let target = "file:///tmp/native-fixture/defs/target%20%23%C3%A9.rs";
    let version = LSP_LOG.with(|log| {
        log.borrow()
            .as_ref()
            .and_then(|log| {
                log.peek()
                    .iter()
                    .rev()
                    .filter_map(|entry| serde_json::from_str::<serde_json::Value>(entry).ok())
                    .find(|entry| {
                        entry["params"]["textDocument"]["uri"] == uri
                            && (entry["method"] == "textDocument/didChange"
                                || entry["method"] == "textDocument/didOpen")
                    })
            })
            .map(|entry| entry["params"]["textDocument"]["version"].clone())
            .unwrap_or(serde_json::json!(1))
    });
    let result = match mode {
        1 => serde_json::json!({"changes":{uri:edits}}),
        5 | 6 | 8 | 12 => {
            let target_uri = match mode {
                8 => "file:///tmp/native-fixture-other/target.rs",
                12 => "file:///tmp/native-fixture/missing.rs",
                _ => target,
            };
            serde_json::json!({"documentChanges":[{"textDocument":{"uri":uri,"version":null},"edits":edits},{"textDocument":{"uri":target_uri,"version":null},"edits":[edit(1,2,if mode==6 {99} else {7})]}]})
        }
        7 => serde_json::json!({"changes":{uri:[edit(0,2,5),edit(0,3,5)]}}),
        9 => serde_json::json!({"changes":{uri:[edit(0,2,5),{"newText":"broken"}]}}),
        10 | 11 => {
            serde_json::json!({"documentChanges":[{"textDocument":{"uri":uri,"version":if mode==10 {serde_json::json!(-1)} else {version}},"edits":edits}]})
        }
        _ => serde_json::Value::Null,
    };
    let response = if mode == 4 {
        serde_json::json!({"jsonrpc":"2.0","id":request["id"],"error":{"code":-32603,"message":"rename error"}})
    } else {
        serde_json::json!({"jsonrpc":"2.0","id":request["id"],"result":result})
    };
    sender.unbounded_send(response.to_string()).unwrap();
}

fn respond_definition(
    sender: &futures_channel::mpsc::UnboundedSender<String>,
    request: &serde_json::Value,
    mode: u8,
) {
    let same = serde_json::json!({"uri":"file:///tmp/native-fixture/main.rs", "range":{"start":{"line":1,"character":9},"end":{"line":1,"character":10}}});
    let target = "file:///tmp/native-fixture/defs/target%20%23%C3%A9.rs";
    let result = match mode {
        1 => same.clone(),
        2 => {
            serde_json::json!({"uri":target,"range":{"start":{"line":1,"character":2},"end":{"line":1,"character":7}}})
        }
        4 => {
            serde_json::json!({"uri":"file:///tmp/native-fixture-other/a.rs","range":{"start":{"line":0,"character":0}}})
        }
        7 => {
            serde_json::json!({"uri":"file:///tmp/native-fixture/%ZZ.rs","range":{"start":{"line":0,"character":0}}})
        }
        8 => serde_json::json!([same,{"uri":target,"range":{"start":{"line":0,"character":0}}}]),
        9 => {
            serde_json::json!([{"targetUri":target,"targetRange":{"start":{"line":0,"character":0},"end":{"line":2,"character":0}},"targetSelectionRange":{"start":{"line":1,"character":2},"end":{"line":1,"character":7}}}])
        }
        10 => {
            serde_json::json!({"uri":"file:///tmp/native-fixture/missing.rs","range":{"start":{"line":0,"character":0}}})
        }
        _ => serde_json::Value::Null,
    };
    let response = if mode == 5 {
        serde_json::json!({"jsonrpc":"2.0","id":request["id"],"error":{"code":-32603,"message":"definition error"}})
    } else {
        serde_json::json!({"jsonrpc":"2.0","id":request["id"],"result":result})
    };
    sender.unbounded_send(response.to_string()).unwrap();
}
fn definition_nodes(source: &SourceId) -> (Node, Node) {
    let folder = Node {
        id: NodeId::derive(source, "defs"),
        source: source.clone(),
        kind: NodeKind::Directory,
        label: "defs".into(),
        native_key: "defs".into(),
        content: None,
        props: Default::default(),
        version: Version::default(),
    };
    let target = Node {
        id: NodeId::derive(source, "defs/target #é.rs"),
        source: source.clone(),
        kind: NodeKind::File,
        label: "target #é.rs".into(),
        native_key: "defs/target #é.rs".into(),
        content: None,
        props: Default::default(),
        version: Version::default(),
    };
    (folder, target)
}
fn respond_hover(
    sender: &futures_channel::mpsc::UnboundedSender<String>,
    request: &serde_json::Value,
) {
    let line = &request["params"]["position"]["line"];
    let column = &request["params"]["position"]["character"];
    let text = format!("```rust\nhover at {line}:{column}\n```\n\n<safe> docs");
    sender.unbounded_send(serde_json::json!({"jsonrpc":"2.0", "id":request["id"], "result":{"contents":{"kind":"markdown","value":text}}}).to_string()).unwrap();
}
fn fake_lsp(_: String, _: String) -> moonkale_lsp::LspTransportFuture {
    Box::pin(async {
        let (sender, incoming) = futures_channel::mpsc::unbounded();
        LSP_SEND.with(|value| *value.borrow_mut() = Some(sender.clone()));
        Ok(Box::new(FakeLsp {
            sender,
            incoming: Some(incoming),
        }) as Box<dyn moonkale_lsp::LspTransport>)
    })
}
fn publish_fixture(version: i32, clear: bool) {
    let diagnostics = if clear {
        serde_json::json!([])
    } else {
        serde_json::json!([
            {"range":{"start":{"line":1,"character":7},"end":{"line":1,"character":9}},"severity":1,"message":"Unicode error <safe>"},
            {"range":{"start":{"line":2,"character":0},"end":{"line":2,"character":0}},"severity":2,"message":"Empty range warning"}
        ])
    };
    LSP_SEND.with(|value| { if let Some(sender) = value.borrow().as_ref() {
        sender.unbounded_send(serde_json::json!({"jsonrpc":"2.0","method":"textDocument/publishDiagnostics","params":{"uri":"file:///tmp/native-fixture/main.rs","version":version,"diagnostics":diagnostics}}).to_string()).unwrap();
    } });
}
fn main() {
    #[cfg(feature = "desktop")]
    {
        let title = if std::env::var_os("MOONKALE_RENDERER_PROBE").is_some() {
            "Moonkale renderer probe"
        } else {
            "Dioxus App"
        };
        dioxus::LaunchBuilder::new()
            .with_cfg(
                dioxus::desktop::Config::new().with_window(
                    dioxus::desktop::WindowBuilder::new()
                        .with_title(title)
                        .with_always_on_top(cfg!(debug_assertions)),
                ),
            )
            .launch(DesktopApp);
    }
    #[cfg(not(feature = "desktop"))]
    dioxus::launch(App);
}

#[cfg(feature = "desktop")]
#[component]
fn DesktopApp() -> Element {
    let desktop = dioxus::desktop::use_window();
    rsx! {
        // cargo run does not bundle asset! URLs; use the exact production CSS.
        style { {include_str!("../../../../../code-view/assets/panel.css")} }
        style { {include_str!("../../../../../code-view/assets/native.css")} }
        style { ".desktop-fixture > button, .desktop-fixture > select {{ display: none; }} .desktop-fixture .canonical {{ display: block; white-space: pre-wrap; }} .desktop-fixture .harness-editor {{ width: 100%; height: 420px; }}" }
        div {
            class: "desktop-fixture",
            onmounted: move |_| {
                if let Some(path) = std::env::var_os("MOONKALE_PROPORTIONAL_PROBE") {
                    spawn(async move {
                        let mut eval = document::eval(include_str!("proportional-probe.js"));
                        let result: serde_json::Value = eval.recv().await.expect("native proportional probe");
                        std::fs::write(path, result.to_string()).expect("write proportional probe");
                    });
                }
                if let Some(path) = std::env::var_os("MOONKALE_RENDERER_PROBE") {
                    // The visual probe must exercise a shown window, not a
                    // background/minimized window restored by the WM.
                    desktop.window.set_minimized(false);
                    desktop.window.set_visible(true);
                    desktop.window.set_focus();
                    spawn(async move {
                        let script = if std::env::var_os("MOONKALE_ISOLATION_PROBE").is_some() {
                            include_str!("isolation-probe.js")
                        } else { include_str!("renderer-probe.js") };
                        let mut eval = document::eval(script);
                        eval.send(serde_json::json!({"proportional": std::env::var_os("MOONKALE_OS_PROPORTIONAL").is_some(), "presentation": std::env::var_os("MOONKALE_OS_PRESENTATION").is_some(), "consumed": std::env::var_os("MOONKALE_OS_CONSUMED_RUNS").is_some(), "multiline": std::env::var_os("MOONKALE_OS_MULTILINE").is_some()})).expect("send renderer mode");
                        let result: serde_json::Value = eval.recv().await.expect("native renderer probe");
                        std::fs::write(&path, result.to_string()).expect("write renderer probe");
                        let events_path = format!("{}.events.json", std::path::Path::new(&path).display());
                        while let Ok(events) = eval.recv::<serde_json::Value>().await {
                            let report_path = if events.get("state").is_some() {
                                format!("{}.state.json", std::path::Path::new(&path).display())
                            } else { events_path.clone() };
                            std::fs::write(&report_path, events.to_string()).expect("write native input observations");
                        }
                    });
                }
            },
            App {}
        }
    }
}
fn open(_: String, _: OpenOptions) -> OpenFolderFuture {
    Box::pin(async { Err(SourceError::NotFound) })
}
fn attach(descriptor: SourceDescriptor) -> AttachFuture {
    Box::pin(async move {
        if descriptor.family == SourceFamily::Index {
            return Ok(Arc::new(WikiIndex) as Arc<dyn Source>);
        }
        Ok(Arc::new(MemoryFile {
            descriptor,
            saved: Mutex::new((
                "fn main() {\n    // 😀中 Unicode\n}\n".into(),
                Version::default(),
            )),
        }) as Arc<dyn Source>)
    })
}
struct FixtureBus {
    eval: document::Eval,
}
impl moonkale_ext_api::SessionBus for FixtureBus {
    fn send(&self, message: moonkale_ext_api::SessionMessage) {
        let _ = self.eval.send(message);
    }
}
fn fixture_bus(
    deliver: Callback<moonkale_ext_api::SessionMessage>,
) -> std::rc::Rc<dyn moonkale_ext_api::SessionBus> {
    let eval = document::eval(
        r#"
        const bus = new BroadcastChannel("moonkale-native-fixture-session");
        bus.onmessage = e => dioxus.send(e.data);
        for (;;) { bus.postMessage(await dioxus.recv()); }
    "#,
    );
    let mut incoming = eval;
    spawn(async move {
        while let Ok(message) = incoming.recv().await {
            deliver.call(message);
        }
    });
    std::rc::Rc::new(FixtureBus { eval })
}
fn wiki_node() -> Node {
    let source = SourceId::new("folder:/tmp/native-fixture");
    Node {
        id: NodeId::derive(&source, "Note é.md"),
        source,
        kind: NodeKind::File,
        label: "Note é.md".into(),
        native_key: "Note é.md".into(),
        content: None,
        props: Default::default(),
        version: Version::default(),
    }
}
struct WikiIndex;
#[cfg_attr(not(target_arch = "wasm32"), moonkale_core::async_trait)]
#[cfg_attr(target_arch = "wasm32", moonkale_core::async_trait(?Send))]
impl Source for WikiIndex {
    fn id(&self) -> SourceId {
        SourceId::new("index:wiki-fixture")
    }
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id(),
            root: NodeId::derive(&self.id(), "root"),
            display_name: "Wiki fixture".into(),
            family: SourceFamily::Index,
            capabilities: Default::default(),
        }
    }
    async fn query(&self, _: Query) -> Result<QueryResult, SourceError> {
        while WIKI_DELAY.with(|hold| hold.get()) {
            futures_timer::Delay::new(std::time::Duration::from_millis(10)).await;
        }
        Ok(if WIKI_INDEX.with(|enabled| enabled.get()) {
            QueryResult::single(wiki_node())
        } else {
            QueryResult::default()
        })
    }
    async fn fetch_text(&self, _: NodeId) -> Result<(String, Version), SourceError> {
        Err(SourceError::NotFound)
    }
    async fn apply(&self, _: Transaction) -> Result<Applied, SourceError> {
        Err(SourceError::NotFound)
    }
}
struct MemoryFile {
    descriptor: SourceDescriptor,
    saved: Mutex<(String, Version)>,
}
#[cfg_attr(not(target_arch = "wasm32"), moonkale_core::async_trait)]
#[cfg_attr(target_arch = "wasm32", moonkale_core::async_trait(?Send))]
impl Source for MemoryFile {
    fn id(&self) -> SourceId {
        self.descriptor.id.clone()
    }
    fn descriptor(&self) -> SourceDescriptor {
        self.descriptor.clone()
    }
    async fn query(&self, query: Query) -> Result<QueryResult, SourceError> {
        let (folder, target) = definition_nodes(&self.id());
        match query {
            Query::Children(id) if id == self.descriptor.root => {
                let mut result = QueryResult::single(folder);
                let mut page = wiki_node();
                page.source = self.id();
                page.id = NodeId::derive(&page.source, &page.native_key);
                result.nodes.push(page);
                Ok(result)
            }
            Query::Node(id) => Ok(WIKI_CREATED.with(|created| {
                created
                    .borrow()
                    .get(&id)
                    .map(|(node, _)| QueryResult::single(node.clone()))
                    .unwrap_or_default()
            })),
            Query::Children(id) if id == folder.id => Ok(QueryResult::single(target)),
            _ => Ok(QueryResult::default()),
        }
    }
    async fn fetch_text(&self, node: NodeId) -> Result<(String, Version), SourceError> {
        if let Some(text) =
            WIKI_CREATED.with(|created| created.borrow().get(&node).map(|(_, text)| text.clone()))
        {
            return Ok((text, Version::default()));
        }
        if node == definition_nodes(&self.id()).1.id {
            LSP_LOG.with(|log| { if let Some(mut log) = *log.borrow() { log.write().push(serde_json::json!({"method":"fixture/fetchDefinition", "params":{"source":self.id().as_str()}}).to_string()); } });
            while DEFINITION_LOAD.with(|hold| hold.get()) {
                futures_timer::Delay::new(std::time::Duration::from_millis(10)).await;
            }
            let text = if self.id().as_str() == "folder:/tmp/native-fixture" {
                "// target\r\n😀value\r\n"
            } else {
                "WRONG FOLDER\r\n"
            };
            return Ok((text.into(), Version::default()));
        }
        Ok(self.saved.lock().unwrap().clone())
    }
    async fn apply(&self, tx: Transaction) -> Result<Applied, SourceError> {
        let mut saved = self.saved.lock().unwrap();
        let mut results = vec![];
        for op in tx.ops {
            if let Op::CreateText { name, text, .. } = &op {
                let mut node = wiki_node();
                node.native_key = name.clone();
                node.label = name.clone();
                node.source = self.id();
                node.id = NodeId::derive(&node.source, name);
                WIKI_CREATED.with(|created| {
                    created
                        .borrow_mut()
                        .insert(node.id, (node.clone(), text.clone()))
                });
                results.push(OpResult::Ok {
                    node: node.id,
                    version: node.version,
                });
            }
            if let Op::WriteText {
                node,
                expected,
                patch,
            } = op
            {
                assert_eq!(expected, saved.1);
                saved.0 = patch.apply(&saved.0)?;
                saved.1 = Version(saved.1 .0 + 1);
                results.push(OpResult::Ok {
                    node,
                    version: saved.1,
                });
            }
        }
        Ok(Applied { results })
    }
}
#[component]
fn App() -> Element {
    let mut show_proportional = use_signal(|| false);
    let mut layout_fixture = use_signal(moonkale_code_view::LayoutFixture::default);
    use_context_provider(|| layout_fixture);
    let lsp = use_hook(moonkale_code_view::lsp::LspManager::new);
    let lsp_log = use_hook(|| Signal::new_in_scope(Vec::<String>::new(), ScopeId::ROOT));
    use_hook(move || LSP_LOG.with(|value| *value.borrow_mut() = Some(lsp_log)));
    let ws = use_hook(|| {
        let mut ws = Workspace::new(WorkspaceConfig {
            folders: FolderAccess {
                open,
                pick: None,
                attach,
                reopen_last: false,
                openers: &moonkale_core::source::opener::NO_OPENERS,
            },
            processes: moonkale_ext_api::Processes {
                lsp: Some(fake_lsp),
                ..Default::default()
            },
            persistence: Default::default(),
            network: Default::default(),
            runtimes: Default::default(),
            services: &[],
        });
        ws.settings
            .user
            .with_mut(|file| file.editor.wrap = Some(true));
        ws.settings
            .resolved
            .with_mut(|settings| settings.editor.wrap = true);
        let source = SourceId::new("folder:/tmp/native-fixture");
        let node = Node {
            id: NodeId::derive(&source, "main.rs"),
            source: source.clone(),
            kind: NodeKind::File,
            label: "main.rs".into(),
            native_key: "main.rs".into(),
            content: None,
            props: Default::default(),
            version: Version(0),
        };
        let text = "fn main() {\n    // 😀中 Unicode\n}\n".to_string();
        let decoy = SourceId::new("folder:/tmp/decoy");
        ws.add_source(Arc::new(MemoryFile {
            descriptor: SourceDescriptor {
                id: decoy.clone(),
                root: NodeId::derive(&decoy, "root"),
                display_name: "decoy".into(),
                family: SourceFamily::Folder,
                capabilities: Default::default(),
            },
            saved: Mutex::new(("WRONG FOLDER".into(), Version::default())),
        }));
        ws.add_source(Arc::new(MemoryFile {
            descriptor: SourceDescriptor {
                id: source,
                root: node.id,
                display_name: "fixture".into(),
                family: SourceFamily::Folder,
                capabilities: Default::default(),
            },
            saved: Mutex::new((text.clone(), node.version)),
        }));
        ws.add_source(Arc::new(WikiIndex));
        let doc = Signal::new_in_scope(
            Document::new(node.clone(), text, node.version),
            ScopeId::ROOT,
        );
        ws.docs.open.with_mut(|docs| docs.push((node.id, doc)));
        ws.docs.active.set(Some(node.id));
        let mut python = node.clone();
        python.id = NodeId::derive(&python.source, "nested.py");
        python.label = "nested.py".into();
        python.native_key = "nested.py".into();
        let python_doc = Signal::new_in_scope(Document::new(python.clone(), "def outer(): # outer block\n    if ready: # nested block\n        while work:\n            run()\n    finish()\n".into(), python.version), ScopeId::ROOT);
        ws.docs
            .open
            .with_mut(|docs| docs.push((python.id, python_doc)));
        for (name, text) in [
            ("nested.html", "<main>\r\n  <section>\r\n    😀 needle\r\n  </section>\r\n</main>\r\n"),
            ("nested.md", "# Outer\r\nintro\r\n## Inner\r\n```rust\r\nneedle 😀\r\n```\r\n# Next\r\nlast\r\n"),
            ("nested.lean", "namespace Demo\r\nsection Inner\r\ntheorem outer : True := by\r\n  have h : True := by\r\n    trivial -- needle 😀\r\n  exact h\r\nend Inner\r\nend Demo\r\ndef next : Nat :=\r\n  42\r\n"),
            ("nested.jl", "module Demo\r\nfunction outer(x)\r\n    if x\r\n        println(\"needle 😀\") # end\r\n    end\r\nend\r\nend\r\n"),
        ] {
            let mut file = node.clone();
            file.id = NodeId::derive(&file.source, name);
            file.label = name.into();
            file.native_key = name.into();
            let document = Signal::new_in_scope(Document::new(file.clone(), text.into(), file.version), ScopeId::ROOT);
            ws.docs.open.with_mut(|docs| docs.push((file.id, document)));
        }
        ws
    });
    let node = use_hook(|| ws.docs.open.peek()[0].0);
    let doc = use_hook(|| ws.document(node).unwrap());
    let mut mounted = use_signal(|| true);
    let mut dock_right = use_signal(|| false);
    let mut duplicate = use_signal(|| false);
    let mut show_python = use_signal(|| false);
    let python = use_hook(|| ws.docs.open.peek()[1].0);
    let mut show_language = use_signal(|| None::<NodeId>);
    let languages: Vec<_> = ws
        .docs
        .open
        .read()
        .iter()
        .filter(|(id, _)| *id != node && *id != python)
        .map(|(id, doc)| (*id, doc.peek().node.label.clone()))
        .collect();
    let indent_fixtures: Vec<_> = ws
        .docs
        .open
        .read()
        .iter()
        .filter(|(id, _)| *id != node)
        .filter_map(|(id, doc)| {
            let name = doc.peek().node.label.clone();
            let text = match name.as_str() {
                "nested.py" => "if ready: # 😀\r\n    run()\r\n",
                "nested.html" => "<main></main>\r\n",
                "nested.jl" => "function outer(x) # 😀\r\nend\r\n",
                _ => return None,
            };
            Some((*id, name, text))
        })
        .collect();
    let closing_fixtures: Vec<_> = ws.docs.open.peek()[2..]
        .iter()
        .filter_map(|(id, doc)| {
            let name = doc.peek().node.label.clone();
            let text = match name.as_str() {
                "nested.html" => "<main>\r\n  <section>\r\n      </section",
                "nested.jl" => "module Demo\r\n    function outer(x)\r\n        en",
                _ => return None,
            };
            Some((*id, name, text))
        })
        .collect();
    let language_text = show_language()
        .and_then(|id| ws.document(id))
        .map(|doc| doc.read().text.clone())
        .unwrap_or_default();
    let definition_active =
        (*ws.docs.active.read()).filter(|id| *id == definition_nodes(&doc.peek().node.source).1.id);
    let definition_text = definition_active
        .and_then(|id| ws.document(id))
        .map(|doc| doc.read().text.clone())
        .unwrap_or_default();
    let definition_source = definition_active
        .and_then(|id| ws.document(id))
        .map(|doc| doc.read().node.source.as_str().to_string())
        .unwrap_or_default();
    let status = ws.shell.status.read().clone();
    let text = doc.read().text.clone();
    let saved = doc.read().saved.clone();
    // Opt-in observation of canonical Workspace text for native OS-key tests.
    // No file is written in normal fixture runs or the production component.
    #[cfg(feature = "desktop")]
    use_effect(move || {
        let text = doc.read().text.clone();
        if let Some(path) = std::env::var_os("MOONKALE_NATIVE_REPORT") {
            std::fs::write(&path, text).expect("write native acceptance report");
            spawn(async move {
                let mut eval = document::eval(
                    r#"
                    await new Promise(resolve => setTimeout(resolve, 100));
                    const cell = document.querySelector('.primary [data-line="0"] .mk-editor-core-cell');
                    if (!cell) { dioxus.send(null); await dioxus.recv(); return; }
                    const rect = cell.getBoundingClientRect();
                    dioxus.send({x: rect.x, y: rect.y, width: rect.width, height: rect.height});
                    await dioxus.recv();
                "#,
                );
                let mut geometry: serde_json::Value =
                    eval.recv().await.expect("native fixture geometry");
                let _ = eval.send(serde_json::Value::Null);
                if !geometry.is_null() {
                    geometry["pid"] = serde_json::json!(std::process::id());
                    let path = format!("{}.geometry.json", std::path::Path::new(&path).display());
                    std::fs::write(path, geometry.to_string()).expect("write native geometry");
                }
            });
        }
    });
    rsx! {
        style { ":root {{ --mk-code-bg: #fff; --mk-code-ink: #222; --mk-muted: #777; --mk-code-selection: #aaccee; }} .harness-editor {{height: 450px; width: 950px;}} .canonical,.saved {{display:none}}" }
        for (id, name) in languages {
            button { "data-language-fixture": "{name}", onclick: move |_| {
                let mut ws = ws;
                if show_language() == Some(id) { show_language.set(None); ws.docs.active.set(Some(node)); }
                else { show_language.set(Some(id)); ws.docs.active.set(Some(id)); }
            }, "{name}" }
        }
        for (id, name, text) in indent_fixtures {
            button { "data-indent-fixture": "{name}", onclick: move |_| {
                if let Some(mut doc) = ws.document(id) { doc.write().text = text.into(); }
                show_language.set(Some(id));
                let mut ws = ws;
                ws.docs.active.set(Some(id));
            }, "Indent {name}" }
        }
        for (id, name, text) in closing_fixtures {
            button { "data-closing-fixture": "{name}", onclick: move |_| {
                if let Some(mut doc) = ws.document(id) { doc.write().text = text.into(); }
                show_language.set(Some(id));
                let mut ws = ws;
                ws.docs.active.set(Some(id));
            }, "Close {name}" }
        }
        pre { class: "lsp-log", hidden: true, "{serde_json::to_string(&*lsp_log.read()).unwrap()}" }
        for (mode,name) in [(1u8,"normal"),(2,"slow"),(3,"empty"),(4,"error"),(5,"cross"),(6,"invalid"),(7,"malformed"),(8,"many")] {
            button { "data-actions-mode":name, onclick:move |_| ACTIONS_MODE.with(|value| value.set(mode)), "Actions {name}" }
        }
        for (mode,name) in [(1u8,"normal"),(2,"slow"),(3,"empty"),(4,"error"),(5,"cross"),(6,"fold"),(7,"missing"),(8,"malformed")] {
            button { "data-references-mode":name, onclick:move |_| REFERENCES_MODE.with(|value| value.set(mode)), "References {name}" }
        }
        for (mode,name) in [(1u8,"normal"),(2,"slow"),(3,"error"),(4,"invalid"),(5,"command")] {
            button { "data-resolve-mode":name, onclick:move |_| RESOLVE_MODE.with(|value| value.set(mode)), "Resolve {name}" }
        }
        button { id:"tools-release", onclick:move |_| TOOLS_DELAYED.with(|pending| { for (sender,request) in pending.borrow_mut().drain(..) { respond_tools(&sender,&request,1); } }), "Release tools replies" }
        button { id:"tools-diagnostics", onclick:move |_| LSP_SEND.with(|sender| { if let Some(sender)=sender.borrow().as_ref() { sender.unbounded_send(serde_json::json!({"jsonrpc":"2.0","method":"textDocument/publishDiagnostics","params":{"uri":"file:///tmp/native-fixture/main.rs","diagnostics":[{"range":{"start":{"line":0,"character":2},"end":{"line":0,"character":5}},"severity":1,"message":"Fix old"}]}}).to_string()).unwrap(); } }), "Tool diagnostics" }
        for (mode, name) in [(1u8,"normal"),(2,"slow"),(3,"empty"),(4,"error"),(5,"cross"),(6,"invalid"),(7,"overlap"),(8,"outside"),(9,"malformed"),(10,"stale-version"),(11,"versioned"),(12,"missing")] {
            button { "data-rename-mode": name, onclick: move |_| RENAME_MODE.with(|value| value.set(mode)), "Rename {name}" }
        }
        button { id: "rename-fixture", onclick: move |_| { let mut doc = doc; doc.write().text = "😀old old\r\n".into(); }, "Rename fixture" }
        button { id: "rename-release", onclick: move |_| RENAME_DELAYED.with(|pending| { for (sender, request) in pending.borrow_mut().drain(..) { respond_rename(&sender, &request, 1); } }), "Release rename" }
        button { id: "rename-target-show", onclick: move |_| { let mut ws = ws; ws.docs.active.set(Some(definition_nodes(&doc.peek().node.source).1.id)); }, "Show rename target" }
        button { id: "rename-target-external", onclick: move |_| {
            let id = definition_nodes(&doc.peek().node.source).1.id;
            if let Some(mut target) = ws.document(id) { target.write().text = "// externally refreshed\r\n😀value\r\n".into(); }
        }, "External target edit" }
        button { id: "rename-other-edit", onclick: move |_| { if let Some(mut doc) = ws.document(python) { doc.write().text.push('x'); } }, "Edit other document" }
        div { class: "rename-target-canonical", hidden: true, {ws.document(definition_nodes(&doc.peek().node.source).1.id).map(|doc| doc.read().text.clone()).unwrap_or_default()} }
        for (mode, name) in [(1u8,"same"),(2,"cross"),(3,"slow"),(4,"outside"),(5,"error"),(6,"empty"),(7,"malformed"),(8,"array"),(9,"link"),(10,"missing")] {
            button { "data-definition-mode": name, onclick: move |_| DEFINITION_MODE.with(|value| value.set(mode)), "Definition {name}" }
        }
        button { id: "definition-release", onclick: move |_| DEFINITION_DELAYED.with(|pending| { for (sender, request) in pending.borrow_mut().drain(..) { respond_definition(&sender, &request, 2); } }), "Release definition" }
        button { id: "definition-hold-load", onclick: move |_| DEFINITION_LOAD.with(|hold| hold.set(true)), "Hold definition file load" }
        button { id: "definition-release-load", onclick: move |_| DEFINITION_LOAD.with(|hold| hold.set(false)), "Release definition file load" }
        button { id: "definition-origin", onclick: move |_| { let mut ws = ws; ws.docs.active.set(Some(node)); }, "Definition origin" }
        button { id: "definition-close", onclick: move |_| { let mut ws = ws; ws.close_node(definition_nodes(&doc.peek().node.source).1.id); ws.docs.active.set(Some(node)); }, "Close definition target" }
        div { class: "definition-canonical", hidden: true, "{definition_text}" }
        div { class: "definition-source", hidden: true, "{definition_source}" }
        div { class: "definition-status", hidden: true, "{status}" }
        if let Some(target) = definition_active { div { class: "harness-editor definition-target", RustCodeEditorPanel { ws, node: target, lsp } } }
        for (mode, name) in [(0u8, "empty"), (1, "normal"), (2, "slow"), (3, "error"), (4, "edits"), (6, "many")] {
            button { "data-completion-mode": name, onclick: move |_| COMPLETION_MODE.with(|value| value.set(mode)), "Completion {name}" }
        }
        button { id: "completion-unicode", onclick: move |_| { let mut doc = doc; doc.write().text = "😀pri\r\n".into(); }, "Unicode completion fixture" }
        button { id: "completion-fixture", onclick: move |_| { let mut doc = doc; doc.write().text = "// 😀\r\npri\r\n".into(); }, "Completion fixture" }
        button { id: "completion-release", onclick: move |_| COMPLETION_DELAYED.with(|pending| {
            for (sender, request) in pending.borrow_mut().drain(..) { respond_completion(&sender, &request, 5); }
        }), "Release completion replies" }
        for (mode, name) in [(0u8, "normal"), (1, "slow"), (2, "empty"), (3, "error"), (4, "blank")] {
            button { "data-hover-mode": name, onclick: move |_| HOVER_MODE.with(|value| value.set(mode)), "Hover {name}" }
        }
        button { id: "hover-crlf", onclick: move |_| { let mut doc = doc; doc.write().text = "a\r\n😀中\r\n".into(); }, "Hover CRLF" }
        button { id: "hover-release", onclick: move |_| HOVER_DELAYED.with(|pending| {
            for (sender, request) in pending.borrow_mut().drain(..) { respond_hover(&sender, &request); }
        }), "Release hover replies" }
        button { id: "diagnostics", onclick: move |_| publish_fixture(1, false), "Publish diagnostics" }
        button { id: "diagnostics-current", onclick: move |_| {
            let version = lsp_log.peek().iter().rev().filter_map(|entry| serde_json::from_str::<serde_json::Value>(entry).ok()).find_map(|entry| {
                if entry["params"]["textDocument"]["uri"] == "file:///tmp/native-fixture/main.rs" { entry["params"]["textDocument"]["version"].as_i64() } else { None }
            }).unwrap_or(1) as i32;
            publish_fixture(version, false);
        }, "Publish current diagnostics" }
        button { id: "diagnostics-clear", onclick: move |_| publish_fixture(1, true), "Clear diagnostics" }
        button { id: "closing", onclick: move |_| { let mut doc = doc; doc.write().text = "fn f() { // 😀\r\n    ".into(); }, "Closing token" }
        button { id: "closing-skip", onclick: move |_| { let mut doc = doc; doc.write().text = "fn f() { // 😀\r\n    }\r\n".into(); }, "Existing closing token" }
        button { id: "syntax", onclick: move |_| { let mut doc = doc; doc.write().text = "fn f() { // 😀 comment\r\n}\r\n".into(); }, "Syntax indentation" }
        button { id: "syntax-string", onclick: move |_| { let mut doc = doc; doc.write().text = "let text = \"{\";\r\n".into(); }, "String indentation" }
        div { class: "language-canonical", hidden: true, "{language_text}" }
        if let Some(id) = show_language() { div { class: "harness-editor language", RustCodeEditorPanel { ws, node: id, lsp } } }
        div { id: "active-source", hidden: true, "{ws.docs.active.read().and_then(|id| ws.document(id)).map(|doc| doc.peek().node.source.to_string()).unwrap_or_default()}" }
        button { id: "wiki-code", onclick: move |_| { let mut target = doc; target.write().text = "😀 [[Note é]]".into(); }, "Wiki in code" }
        div { id: "active-key", hidden: true, "{ws.docs.active.read().and_then(|id| ws.document(id)).map(|doc| doc.peek().node.native_key.clone()).unwrap_or_default()}" }
        button { id: "wiki", onclick: move |_| {
            let mut ws = ws;
            let id = NodeId::derive(&doc.peek().node.source, "nested.md");
            let mut target = ws.document(id).unwrap();
            target.write().text = "😀中 [[Note é#Heading|alias]] [[Missing]]\r\n".into();
            show_language.set(Some(id)); ws.docs.active.set(Some(id));
        }, "Wiki links" }
        button { id: "wiki-completion", onclick: move |_| {
            let mut ws = ws;
            let id = NodeId::derive(&doc.peek().node.source, "nested.md");
            let mut target = ws.document(id).unwrap(); target.write().text = "😀 [[No]]\r\n".into();
            show_language.set(Some(id)); ws.docs.active.set(Some(id));
        }, "Wiki completion" }
        button { id: "wiki-index-toggle", onclick: move |_| { WIKI_INDEX.with(|enabled| enabled.set(!enabled.get())); let mut epoch = ws.sources.graph_epoch; epoch.with_mut(|epoch| *epoch += 1); }, "Toggle wiki index" }
        button { id: "wiki-delay", onclick: move |_| WIKI_DELAY.with(|hold| hold.set(true)), "Hold wiki lookup" }
        button { id: "wiki-release", onclick: move |_| WIKI_DELAY.with(|hold| hold.set(false)), "Release wiki lookup" }
        button { id: "python", onclick: move |_| show_python.toggle(), "Python fixture" }
        button { id: "mount", onclick: move |_| mounted.toggle(), "Toggle mount" }
        button { id: "presence", onclick: move |_| {
            let mut members = ws.session.presence;
            let key = doc.peek().node.native_key.clone();
            members.set(vec![
                moonkale_ext_api::presence::Member { window: "peer".into(), name: "Alice Bob".into(), active: Some(key.clone()), line: Some(1) },
                moonkale_ext_api::presence::Member { window: ws.session.window.peek().to_string(), name: "Local User".into(), active: Some(key), line: Some(0) },
                moonkale_ext_api::presence::Member { window: "other-file".into(), name: "Wrong File".into(), active: Some("elsewhere.rs".into()), line: Some(0) },
            ]);
        }, "Presence fixture" }
        button { id: "presence-move", onclick: move |_| {
            let mut members = ws.session.presence;
            members.with_mut(|members| { if let Some(peer) = members.first_mut() { peer.line = Some(0); } });
        }, "Move presence" }
        button { id: "presence-clear", onclick: move |_| { let mut members = ws.session.presence; members.set(Vec::new()); }, "Clear presence" }
        button { id: "duplicate", onclick: move |_| duplicate.toggle(), "Toggle duplicate" }
        button { id: "undo", onclick: move |_| { let mut ws = ws; ws.dispatch(Command::Undo); }, "Shell undo" }
        button { id: "folds", onclick: move |_| { let mut doc = doc; doc.write().text = "fn main() {\n    if true {\n        let text = \"} 😀\"; // {\n    }\n}\n".into(); }, "Fold document" }
        button { id: "fold-all", onclick: move |_| { let mut ws = ws; ws.dispatch(Command::Editor(EditorAction::FoldAll)); }, "Shell fold all" }
        button { id: "unfold-all", onclick: move |_| { let mut ws = ws; ws.dispatch(Command::Editor(EditorAction::UnfoldAll)); }, "Shell unfold all" }
        button { id: "comment", onclick: move |_| { let mut ws = ws; ws.dispatch(Command::Editor(EditorAction::ToggleComment)); }, "Shell comment" }
        button { id: "redo", onclick: move |_| { let mut ws = ws; ws.dispatch(Command::Redo); }, "Shell redo" }
        button { id: "replace", onclick: move |_| { let mut doc = doc; doc.write().text = "external 😀\n中 update\n".into(); }, "Replace" }
        button { id: "regex", onclick: move |_| { let mut doc = doc; doc.write().text = "😀 猫=12\r\n中=34\r\n猫=56\r\n".into(); }, "Regex fixture" }
        button { id: "prefs", onclick: move |_| { let mut doc = doc; doc.write().text = "fn main() {}\r\n".into(); }, "Preference fixture" }
        button { id: "crlf", onclick: move |_| { let mut doc = doc; doc.write().text = "a\r\nb\r\n".into(); }, "CRLF" }
        button { id: "large", onclick: move |_| { let mut doc = doc; doc.write().text = format!("//{}", "x".repeat(2_999_998)); }, "Large line" }
        div { id: "session-state", hidden: true,
            "data-peers": ws.session.peers.read().len().to_string(),
            "data-drop": ws.session.foreign_drag.read().is_some().to_string(),
            "data-primary-open": ws.docs.open.read().iter().any(|(id, _)| *id == node).to_string(),
        }
        div { id: "active-canonical", hidden: true, "{ws.docs.active.read().and_then(|id| ws.document(id)).map(|doc| doc.read().text.clone()).unwrap_or_default()}" }
        button { id: "integration-connect", onclick: move |_| {
            let deliver = Callback::new(move |message| { spawn(async move { ws.handle_message(message).await; }); });
            let mut ws = ws; ws.connect_bus(fixture_bus(deliver));
        }, "Connect session" }
        button { id: "integration-drag", onclick: move |_| { let mut ws = ws; ws.start_drag_from_tab(&format!("wb-tab-{node}")); ws.end_drag(); }, "Offer tab move" }
        button { id: "integration-accept", onclick: move |_| { spawn(async move { ws.accept_drop().await.unwrap(); }); }, "Accept tab move" }
        button { id: "integration-close-main", onclick: move |_| { let ws = ws; ws.close_node(node); }, "Close main" }
        div { id: "workspace-context", hidden: true,
            "data-line": (*ws.docs.cursor.read()).map(|(line, _)| line.to_string()),
            "data-col": (*ws.docs.cursor.read()).map(|(_, col)| col.to_string()),
            "data-word": ws.cursor_word().unwrap_or_default(),
            "data-selection": (*ws.docs.selection.read()).map(|(a, h)| format!("{a}:{h}")),
            "data-reveal-seq": (*ws.docs.reveal.read()).map(|reveal| reveal.seq.to_string()),
        }
        button { id: "integration-document", onclick: move |_| {
            let mut target = doc;
            target.write().text = (0..240).map(|i| format!("😀word{i} tail\r\n")).collect();
        }, "Long reveal document" }
        button { id: "integration-reveal", onclick: move |_| {
            let from = doc.peek().node.clone(); spawn(async move { ws.reveal(from, 210, 4).await.unwrap(); });
        }, "Workspace reveal" }
        button { id: "integration-replace-reveal", onclick: move |_| {
            let mut target = doc;
            target.write().text = (0..240).map(|i| format!("😀fresh{i} tail\r\n")).collect();
            let from = target.peek().node.clone(); spawn(async move { ws.reveal(from, 180, 4).await.unwrap(); });
        }, "Replace and reveal" }
        button { id: "integration-clamp", onclick: move |_| {
            let from = doc.peek().node.clone(); spawn(async move { ws.reveal(from, u32::MAX, u32::MAX).await.unwrap(); });
        }, "Reveal past end" }
        button { id: "integration-agent", onclick: move |_| {
            let from = doc.peek().node.clone(); spawn(async move { ws.replace_in_file(&from, "fresh", "agent").await.unwrap(); });
        }, "Workspace replace" }
        button { id: "integration-main", onclick: move |_| { let from = doc.peek().node.clone(); spawn(async move { ws.open_node(from).await.unwrap(); }); }, "Activate main" }
        button { id: "integration-python", onclick: move |_| { show_python.set(true); let from = ws.document(python).unwrap().peek().node.clone(); spawn(async move { ws.open_node(from).await.unwrap(); }); }, "Activate Python" }
        button { id: "integration-reload", onclick: move |_| { spawn(async move { ws.reload(node).await.unwrap(); }); }, "Workspace reload" }
        button { id: "integration-dock", onclick: move |_| dock_right.toggle(), "Move dock" }
        button { id: "layout-navigation-document", onclick: move |_| { let mut doc = doc; doc.write().text = format!("{}\nnext row\n", "Wi 😀 é 中\tTabs ".repeat(24)); }, "Measured navigation document" }
        button { id: "layout-presentation", onclick: move |_| {
            let mut doc=doc; doc.write().text=format!("Wi {} [[hidden]] [[chip]] {}\nnext row\n", "x".repeat(42), "é 👩🏽‍💻 中 tail ".repeat(3));
            layout_fixture.set(moonkale_code_view::LayoutFixture {proportional_line:Some(0),block_height:72.0,presentation:true,..Default::default()});
        }, "Replacement/widget fixture" }
        button { id: "layout-multiline", onclick: move |_| {
            let mut doc=doc; doc.write().text="Wi [[hidden:a\n\nb]] gap\n[[chip:c\nd]] tail\nnext row\n".to_string();
            layout_fixture.set(moonkale_code_view::LayoutFixture {proportional_line:Some(0),block_height:72.0,presentation:true,..Default::default()});
        }, "Multiline replacements" }
        button { id: "layout-consumed-runs", onclick: move |_| {
            let mut doc=doc; doc.write().text=format!("Wi [[hidden:{}]] [[chip:{}]] tail\nnext row\n", "x".repeat(100), "y".repeat(100));
            layout_fixture.set(moonkale_code_view::LayoutFixture {proportional_line:Some(0),block_height:72.0,presentation:true,..Default::default()});
        }, "Consumed source runs" }
        button { id: "layout-unicode-document", onclick: move |_| { let mut doc = doc; doc.write().text = format!("{}\nx\n\na\t中é👩🏽‍💻\n", "Wi é 👩🏽‍💻 🇩🇪 한글 क नमस्ते 中\t ".repeat(10)); }, "Unicode navigation document" }
        button { id: "layout-wrapped-document", onclick: move |_| { let mut doc = doc; doc.write().text = format!("//{}\n{}", "x".repeat(2000), (1..200).map(|i| format!("// row {i} 😀\n")).collect::<String>()); }, "Wrapped layout document" }
        button { id: "layout-document", onclick: move |_| { let mut doc = doc; doc.write().text = (0..200).map(|i| format!("// row {i} 😀\n")).collect(); }, "Layout document" }
        button { id: "layout-proportional", onclick: move |_| layout_fixture.set(moonkale_code_view::LayoutFixture { proportional_line: Some(0), ..Default::default() }), "Proportional first line" }
        button { id: "layout-mixed", onclick: move |_| layout_fixture.set(moonkale_code_view::LayoutFixture { line: 1, extra_height: 44.0, block_height: 110.0, ..Default::default() }), "Mixed heights" }
        button { id: "layout-delay", onclick: move |_| layout_fixture.with_mut(|value| value.measurement_delay_ms = 300), "Delay measurement" }
        button { id: "layout-taller", onclick: move |_| layout_fixture.with_mut(|value| value.block_height += 66.0), "Grow block" }
        button { id: "layout-uniform", onclick: move |_| layout_fixture.set(Default::default()), "Uniform heights" }
        button { id: "reveal", onclick: move |_| {
            let mut reveal = ws.docs.reveal;
            let seq = reveal.peek().as_ref().map(|r| r.seq + 1).unwrap_or(1);
            reveal.set(Some(moonkale_ext_api::Reveal { node, line: 1, col: 1, seq }));
        }, "Reveal" }
        button { id: "show-proportional", onclick: move |_| show_proportional.toggle(), "Proportional geometry probe" }
        if show_proportional() { moonkale_code_view::ProportionalGeometryProbe {} }
        div { class: "canonical", "{text}" }
        div { class: "saved", "{saved}" }
        if show_python() { div { class: "harness-editor python", RustCodeEditorPanel { ws, node: python, lsp } } }
        div { class: "dock-left", if mounted() && !dock_right() && ws.docs.open.read().iter().any(|(id, _)| *id == node) { div { class: "harness-editor primary", RustCodeEditorPanel { ws, node, lsp } } } }
        div { class: "dock-right", if mounted() && dock_right() && ws.docs.open.read().iter().any(|(id, _)| *id == node) { div { class: "harness-editor primary", RustCodeEditorPanel { ws, node, lsp } } } }
        if duplicate() { div { class: "harness-editor duplicate", RustCodeEditorPanel { ws, node, lsp } } }
    }
}
