//! A desktop smoke check of the flow panel (the stage 7–10 review's #1
//! and #3): the panel's handlers run natively — outside the browser,
//! where the palette's `web_sys::window()` lookup crashed on the first
//! click, since desktop and Android run Rust handlers outside wasm.
//! The panel renders in a real `VirtualDom` against a real `Workspace`;
//! a palette button's own handler is fired by hand and must add a block
//! to the document. Once the document goes broken on "disk", an undo —
//! which holds a real pre-edit snapshot and would otherwise write the
//! old flow over the file (#11) — must leave the text untouched.

use dioxus::prelude::*;
use dioxus_core::{AttributeValue, ElementId, Template, WriteMutations};
use dioxus_html::{
    Code, Location, Modifiers, PlatformEventData, SerializedHtmlEventConverter,
    SerializedKeyboardData, SerializedMouseData,
};
use moonkale_core::{Node, NodeId, NodeKind, SourceError, SourceId, Version};
use moonkale_ext_api::flow::{BlockKind, Flow, FlowLibrary, Param, ParamKind};
use moonkale_ext_api::{Document, Workspace, WorkspaceConfig};
use std::any::Any;
use std::rc::Rc;

fn open_folder(_: String, _: moonkale_ext_api::OpenOptions) -> moonkale_ext_api::OpenFolderFuture {
    Box::pin(async { Ok(Vec::new()) })
}
fn attach(_: moonkale_core::SourceDescriptor) -> moonkale_ext_api::AttachFuture {
    Box::pin(async { Err(SourceError::NotFound) })
}

fn config() -> WorkspaceConfig {
    WorkspaceConfig {
        folders: moonkale_ext_api::FolderAccess {
            open: open_folder,
            pick: None,
            attach,
            reopen_last: false,
            openers: &moonkale_core::source::opener::NO_OPENERS,
        },
        processes: Default::default(),
        persistence: Default::default(),
        network: Default::default(),
        runtimes: Default::default(),
        services: &[],
    }
}

const FLOW: &str = r#"{"version":1,"blocks":[{"id":"b1","kind":"test/add","x":0,"y":0,"params":{"n":"1"}}],"wires":[]}"#;
const BROKEN: &str = r#"{ not a flow at all"#;

fn library() -> FlowLibrary {
    FlowLibrary {
        id: "test",
        name: "Test",
        blocks: vec![BlockKind {
            id: "add".into(),
            name: "Add".into(),
            category: "math".into(),
            description: "adds".into(),
            inputs: vec![],
            outputs: vec![],
            connections: vec![],
            params: vec![Param {
                name: "n".into(),
                kind: ParamKind::Int,
                default: "1".into(),
            }],
        }],
        codegen: None,
        language: "none",
    }
}

thread_local! {
    static WS: std::cell::Cell<Option<Workspace>> = const { std::cell::Cell::new(None) };
    static NODE: std::cell::Cell<NodeId> = std::cell::Cell::new(NodeId::fresh("node"));
}

#[component]
fn App() -> Element {
    let mut ws = use_context_provider(|| Workspace::new(config()));
    use_hook(move || {
        let source = SourceId::new("test".to_string());
        let node = Node {
            props: Default::default(),
            id: NodeId::derive(&source, "test.flow.json"),
            source,
            kind: NodeKind::File,
            label: "test.flow.json".into(),
            native_key: "test.flow.json".into(),
            content: None,
            version: Version::default(),
        };
        let doc = Signal::new_in_scope(
            Document::new(node.clone(), FLOW.to_string(), Version::default()),
            ScopeId::ROOT,
        );
        ws.docs.open.with_mut(|v| v.push((node.id, doc)));
        ws.contrib.flow_libraries.with_mut(|l| l.push(library()));
        NODE.with(|n| n.set(node.id));
        WS.with(|w| w.set(Some(ws)));
    });
    let node = NODE.with(|n| n.get());
    rsx! { moonkale_editor_flow::FlowPanel { ws, node } }
}

/// Records listeners (`data-kind` buttons, the root's `keydown`);
/// everything else is dropped.
#[derive(Default)]
struct Dom {
    listeners: Vec<(&'static str, ElementId)>,
    data_kind: Vec<ElementId>,
}
impl WriteMutations for Dom {
    fn create_event_listener(&mut self, name: &'static str, id: ElementId) {
        self.listeners.push((name, id));
    }
    fn remove_event_listener(&mut self, name: &'static str, id: ElementId) {
        self.listeners.retain(|l| *l != (name, id));
    }
    fn set_attribute(
        &mut self,
        name: &'static str,
        _: Option<&'static str>,
        _: &AttributeValue,
        id: ElementId,
    ) {
        if name == "data-kind" {
            self.data_kind.push(id);
        }
    }
    // The rest is dropped: only listeners and the tracked attributes matter.
    fn append_children(&mut self, _: ElementId, _: usize) {}
    fn assign_node_id(&mut self, _: &'static [u8], _: ElementId) {}
    fn create_placeholder(&mut self, _: ElementId) {}
    fn create_text_node(&mut self, _: &str, _: ElementId) {}
    fn load_template(&mut self, _: Template, _: usize, _: ElementId) {}
    fn replace_node_with(&mut self, _: ElementId, _: usize) {}
    fn replace_placeholder_with_nodes(&mut self, _: &'static [u8], _: usize) {}
    fn insert_nodes_after(&mut self, _: ElementId, _: usize) {}
    fn insert_nodes_before(&mut self, _: ElementId, _: usize) {}
    fn remove_node(&mut self, _: ElementId) {}
    fn set_node_text(&mut self, _: &str, _: ElementId) {}
    fn push_root(&mut self, _: ElementId) {}
}

async fn settle(dom: &mut VirtualDom, m: &mut Dom) {
    for _ in 0..30 {
        tokio::select! {
            _ = dom.wait_for_work() => {}
            _ = tokio::time::sleep(std::time::Duration::from_millis(10)) => {}
        }
        dom.render_immediate(m);
    }
}

fn doc_text(dom: &VirtualDom) -> String {
    let (ws, node) = dom.in_scope(ScopeId::ROOT, || {
        (
            WS.with(|w| w.get()).expect("workspace"),
            NODE.with(|n| n.get()),
        )
    });
    let doc = ws.document(node).expect("the flow document is open");
    let text = doc.peek().text.clone();
    text
}

#[tokio::test(flavor = "current_thread")]
async fn the_palette_clicks_natively_and_a_broken_document_is_never_written() {
    dioxus_html::set_event_converter(Box::new(SerializedHtmlEventConverter));
    let mut dom = VirtualDom::new(App);
    let mut m = Dom::default();
    dom.rebuild(&mut m);
    settle(&mut dom, &mut m).await;

    // The palette rendered with its own handler per button — natively,
    // with no browser event to read (the review's #1: the old handler
    // called `web_sys::window()` and panicked off the web).
    let button = m
        .data_kind
        .first()
        .copied()
        .expect("the palette button rendered");
    assert!(
        m.listeners
            .iter()
            .any(|(n, i)| *n == "click" && *i == button),
        "the palette button carries no click handler"
    );

    // The click adds the block to the document. The payload travels as
    // the platform event data the handler converts lazily — the click
    // handler itself reads nothing from it (the kind is captured).
    let click: Rc<dyn Any> = Rc::new(PlatformEventData::new(Box::new(
        SerializedMouseData::default(),
    )));
    dom.runtime()
        .handle_event("click", Event::new(click, true), button);
    settle(&mut dom, &mut m).await;
    let flow = Flow::parse(&doc_text(&dom)).expect("the document stays a flow");
    assert_eq!(flow.blocks.len(), 2, "the palette pick added its block");
    assert!(flow.blocks.iter().any(|b| b.kind == "test/add"));

    // The document goes broken on disk; the reload replaces the view.
    let (ws, node) = dom.in_scope(ScopeId::ROOT, || {
        (
            WS.with(|w| w.get()).expect("workspace"),
            NODE.with(|n| n.get()),
        )
    });
    ws.document(node)
        .expect("open")
        .with_mut(|d| d.text = BROKEN.to_string());
    settle(&mut dom, &mut m).await;

    // An undo holds a real snapshot of the pre-click flow; without the
    // broken guard it would write that flow over the file (#11).
    let root = m
        .listeners
        .iter()
        .find(|(n, _)| *n == "keydown")
        .map(|(_, id)| *id)
        .expect("the panel listens for its keys");
    let undo: Rc<dyn Any> = Rc::new(PlatformEventData::new(Box::new(
        SerializedKeyboardData::new(
            Key::Character("z".into()),
            Code::KeyZ,
            Location::Standard,
            false,
            Modifiers::CONTROL,
            false,
        ),
    )));
    dom.runtime()
        .handle_event("keydown", Event::new(undo, true), root);
    settle(&mut dom, &mut m).await;
    let key: Rc<dyn Any> = Rc::new(PlatformEventData::new(Box::new(
        SerializedKeyboardData::new(
            Key::Delete,
            Code::Delete,
            Location::Standard,
            false,
            Modifiers::empty(),
            false,
        ),
    )));
    dom.runtime()
        .handle_event("keydown", Event::new(key, true), root);
    settle(&mut dom, &mut m).await;

    assert_eq!(
        doc_text(&dom),
        BROKEN,
        "a broken document is never written back (#11)"
    );
}
