//! *Connect to Server…* in a real `VirtualDom` (P-116): the dialog closes
//! itself on submit, and the connect it started must survive that — a task
//! owned by the dialog's scope is cancelled when the dialog unmounts.
//! The form's `submit` listener is found through the mutations the dom
//! writes, then fired by hand.

use dioxus::prelude::*;
use dioxus_core::{AttributeValue, ElementId, Template, WriteMutations};
use dioxus_html::{PlatformEventData, SerializedFormData, SerializedHtmlEventConverter};
use moonkale_core::SourceError;
use moonkale_ext_api::{OpenFolderFuture, OpenOptions, ServerClient, Workspace, WorkspaceConfig};
use std::sync::Mutex;

static CONNECTS: Mutex<Vec<(String, Option<String>)>> = Mutex::new(Vec::new());
static OPENED: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn open_folder(path: String, _: OpenOptions) -> OpenFolderFuture {
    Box::pin(async move {
        OPENED.lock().unwrap().push(path);
        Ok(Vec::new())
    })
}
fn attach(_: moonkale_core::SourceDescriptor) -> moonkale_ext_api::AttachFuture {
    Box::pin(async { Err(SourceError::NotFound) })
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
        remote: None,
        agent_sessions: None,
        server: Some(ServerClient {
            connect: |u, t| {
                CONNECTS.lock().unwrap().push((u, t));
                Ok(())
            },
            disconnect: || {},
            active: || None,
        }),
        spawn_program: None,
    }
}

/// Records which element carries which listener; everything else is dropped.
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
    fn remove_node(&mut self, _: ElementId) {}
    fn push_root(&mut self, _: ElementId) {}
}

/// A form event with only a value — what `oninput` and `onsubmit` read.
fn typed(value: &str) -> PlatformEventData {
    PlatformEventData::new(Box::new(SerializedFormData::new(value.into(), Vec::new())))
}

thread_local! {
    static WS: std::cell::Cell<Option<Workspace>> = const { std::cell::Cell::new(None) };
    static OPEN: std::cell::Cell<Option<Signal<bool>>> = const { std::cell::Cell::new(None) };
}

#[component]
fn App() -> Element {
    let ws = use_context_provider(|| Workspace::new(config()));
    let open = use_signal(|| true);
    use_hook(move || {
        WS.with(|w| w.set(Some(ws)));
        OPEN.with(|o| o.set(Some(open)));
    });
    rsx! {
        if open() {
            ui::ServerDialog { open }
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
async fn submit_closes_the_dialog_and_still_connects() {
    dioxus_html::set_event_converter(Box::new(SerializedHtmlEventConverter));
    let mut dom = VirtualDom::new(App);
    let mut m = Listeners::default();
    dom.rebuild(&mut m);
    let inputs: Vec<ElementId> =
        m.0.iter()
            .filter(|(n, _)| *n == "input")
            .map(|(_, id)| *id)
            .collect();
    assert_eq!(inputs.len(), 2, "url and token inputs: {:?}", m.0);
    let form =
        m.0.iter()
            .find(|(n, _)| *n == "submit")
            .expect("a submit listener")
            .1;

    // Type into both fields, then submit.
    for (id, text) in [
        (inputs[0], "http://192.168.178.62:8443/"),
        (inputs[1], "phone-token"),
    ] {
        let data: std::rc::Rc<dyn std::any::Any> = std::rc::Rc::new(typed(text));
        dom.runtime()
            .handle_event("input", Event::new(data, true), id);
    }
    let data: std::rc::Rc<dyn std::any::Any> = std::rc::Rc::new(typed(""));
    dom.runtime()
        .handle_event("submit", Event::new(data, true), form);
    settle(&mut dom, &mut m).await;

    let open = dom.in_scope(ScopeId::ROOT, || OPEN.with(|o| o.get()).unwrap()());
    assert!(!open, "the dialog closed itself");
    let connects = CONNECTS.lock().unwrap().clone();
    assert_eq!(
        connects,
        vec![(
            "http://192.168.178.62:8443".to_string(),
            Some("phone-token".to_string())
        )],
        "connect ran although the dialog that started it is gone"
    );
    assert_eq!(
        OPENED.lock().unwrap().as_slice(),
        [String::new()],
        "the server's folder was opened"
    );
    let ws = dom.in_scope(ScopeId::ROOT, || WS.with(|w| w.get()).unwrap());
    assert_eq!(
        ws.server_link.peek().as_ref().map(|(u, _)| u.clone()),
        Some("http://192.168.178.62:8443".into())
    );
}
