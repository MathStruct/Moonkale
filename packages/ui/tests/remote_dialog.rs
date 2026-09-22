//! *Open Remote Folder…* with saved connections (Milestone 15) in a real
//! `VirtualDom`: type host and path, save them under a name → the user
//! settings hold the connection and the dialog grows a select; pick it,
//! submit → `open_remote` gets the saved host and path. Listeners are found
//! from the mutations the dom writes, in DOM order.

use dioxus::prelude::*;
use dioxus_core::{AttributeValue, ElementId, Template, WriteMutations};
use dioxus_html::{
    PlatformEventData, SerializedFormData, SerializedHtmlEventConverter, SerializedMouseData,
};
use moonkale_core::SourceError;
use moonkale_ext_api::remote::{PhaseSink, RemoteHosts};
use moonkale_ext_api::{OpenFolderFuture, OpenOptions, Workspace, WorkspaceConfig};
use std::sync::Mutex;

static OPENED: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

fn open_folder(_: String, _: OpenOptions) -> OpenFolderFuture {
    Box::pin(async move { Ok(Vec::new()) })
}
fn attach(_: moonkale_core::SourceDescriptor) -> moonkale_ext_api::AttachFuture {
    Box::pin(async { Err(SourceError::NotFound) })
}
fn open_remote(
    host: String,
    path: String,
    _: PhaseSink,
) -> Result<moonkale_ext_api::remote::Opened, String> {
    OPENED.lock().unwrap().push((host, path));
    Err("recorded".into())
}

fn config() -> WorkspaceConfig {
    WorkspaceConfig {
        open_folder,
        pick_folder: None,
        attach_source: attach,
        spawn_terminal: None,
        compile_typst: None,
        spawn_lsp: None,
        llm: None,
        settings_store: None,
        secret_store: None,
        reopen_last_folder: false,
        wasm: None,
        git: None,
        presence: None,
        wasm_module_url: None,
        remote: Some(RemoteHosts {
            open: open_remote,
            hosts: Vec::new,
            at_start: || None,
        }),
        agent_sessions: None,
        server: None,
        spawn_program: None,
    }
}

#[derive(Default)]
struct Listeners(Vec<(&'static str, ElementId)>);
impl WriteMutations for Listeners {
    fn append_children(&mut self, _: ElementId, _: usize) {}
    fn assign_node_id(&mut self, _: &'static [u8], _: ElementId) {}
    fn create_placeholder(&mut self, _: ElementId) {}
    fn create_text_node(&mut self, _: &str, _: ElementId) {}
    fn load_template(&mut self, _: Template, _: usize, _: ElementId) {}
    fn replace_node_with(&mut self, _: ElementId, _: usize) {}
    fn replace_placeholder_with_nodes(&mut self, _: &'static [u8], _: usize) {}
    fn insert_nodes_after(&mut self, _: ElementId, _: usize) {}
    fn insert_nodes_before(&mut self, _: ElementId, _: usize) {}
    fn set_attribute(
        &mut self,
        _: &'static str,
        _: Option<&'static str>,
        _: &AttributeValue,
        _: ElementId,
    ) {
    }
    fn set_node_text(&mut self, _: &str, _: ElementId) {}
    fn create_event_listener(&mut self, name: &'static str, id: ElementId) {
        self.0.push((name, id));
    }
    fn remove_event_listener(&mut self, name: &'static str, id: ElementId) {
        self.0.retain(|l| *l != (name, id));
    }
    fn remove_node(&mut self, id: ElementId) {
        self.0.retain(|l| l.1 != id);
    }
    fn push_root(&mut self, _: ElementId) {}
}

impl Listeners {
    fn of(&self, name: &str) -> Vec<ElementId> {
        self.0
            .iter()
            .filter(|(n, _)| *n == name)
            .map(|(_, id)| *id)
            .collect()
    }
}

fn typed(value: &str) -> PlatformEventData {
    PlatformEventData::new(Box::new(SerializedFormData::new(value.into(), Vec::new())))
}
fn fire(dom: &VirtualDom, name: &str, id: ElementId, value: &str) {
    let data: std::rc::Rc<dyn std::any::Any> = if name == "click" {
        std::rc::Rc::new(PlatformEventData::new(Box::new(
            SerializedMouseData::default(),
        )))
    } else {
        std::rc::Rc::new(typed(value))
    };
    dom.runtime().handle_event(name, Event::new(data, true), id);
}

thread_local! {
    static WS: std::cell::Cell<Option<Workspace>> = const { std::cell::Cell::new(None) };
}

#[component]
fn App() -> Element {
    let ws = use_context_provider(|| Workspace::new(config()));
    let open = use_signal(|| true);
    use_hook(move || WS.with(|w| w.set(Some(ws))));
    rsx! {
        if open() {
            ui::RemoteDialog { open }
        }
    }
}

async fn settle(dom: &mut VirtualDom, m: &mut Listeners) {
    for _ in 0..20 {
        tokio::select! {
            _ = dom.wait_for_work() => {}
            _ = tokio::time::sleep(std::time::Duration::from_millis(10)) => {}
        }
        dom.render_immediate(m);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn saving_and_picking_a_connection() {
    dioxus_html::set_event_converter(Box::new(SerializedHtmlEventConverter));
    let mut dom = VirtualDom::new(App);
    let mut m = Listeners::default();
    dom.rebuild(&mut m);
    // No saved connections yet: no select; inputs are host, path, save-name.
    assert!(m.of("change").is_empty(), "{:?}", m.0);
    let inputs = m.of("input");
    assert_eq!(inputs.len(), 3, "{:?}", m.0);
    fire(&dom, "input", inputs[0], "-p 443 daniel@box");
    fire(&dom, "input", inputs[1], "/srv/code");
    fire(&dom, "input", inputs[2], "box");
    // Clicks in DOM order: backdrop, form, Save, Cancel.
    let clicks = m.of("click");
    assert_eq!(clicks.len(), 4, "{:?}", m.0);
    fire(&dom, "click", clicks[2], "");
    settle(&mut dom, &mut m).await;
    let ws = dom.in_scope(ScopeId::ROOT, || WS.with(|w| w.get()).unwrap());
    let saved = ws.settings.peek().remote_saved.clone();
    assert_eq!(saved.len(), 1);
    assert_eq!(
        (
            saved[0].name.as_str(),
            saved[0].host.as_str(),
            saved[0].path.as_str()
        ),
        ("box", "-p 443 daniel@box", "/srv/code")
    );
    assert!(
        ws.settings_user.peek().remote.saved.len() == 1,
        "the user file holds it"
    );
    // The select appeared; a Forget button too (5 clicks now).
    let select = m.of("change");
    assert_eq!(select.len(), 1, "{:?}", m.0);
    assert_eq!(m.of("click").len(), 5, "{:?}", m.0);
    // Clear the fields, pick the saved one, submit: open_remote gets the saved values.
    let inputs = m.of("input");
    fire(&dom, "input", inputs[0], "");
    fire(&dom, "input", inputs[1], "");
    fire(&dom, "change", select[0], "box");
    let form = m.of("submit")[0];
    fire(&dom, "submit", form, "");
    settle(&mut dom, &mut m).await;
    assert_eq!(
        OPENED.lock().unwrap().clone(),
        vec![("-p 443 daniel@box".to_string(), "/srv/code".to_string())]
    );
}
