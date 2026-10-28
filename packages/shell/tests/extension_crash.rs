//! An extension panel that panics while rendering shows a notice with
//! Reload in its place, instead of taking the window down (2026-10-14).
//! The panic is caught by Dioxus on native builds and turned into an error
//! for the panel's boundary (`shell.rs`, `guarded`).

use dioxus::prelude::*;
use dioxus_core::{AttributeValue, ElementId, Template, WriteMutations};
use moonkale_core::SourceError;
use moonkale_ext_api::{OpenFolderFuture, OpenOptions, Workspace, WorkspaceConfig};

fn open_folder(_: String, _: OpenOptions) -> OpenFolderFuture {
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

/// Collects the text the dom writes, and the listeners it attaches.
#[derive(Default)]
struct Texts(Vec<String>);
impl WriteMutations for Texts {
    fn append_children(&mut self, _: ElementId, _: usize) {}
    fn assign_node_id(&mut self, _: &'static [u8], _: ElementId) {}
    fn create_placeholder(&mut self, _: ElementId) {}
    fn create_text_node(&mut self, t: &str, _: ElementId) {
        self.0.push(t.to_string());
    }
    fn load_template(&mut self, t: Template, _: usize, _: ElementId) {
        fn walk(n: &dioxus_core::TemplateNode, out: &mut Vec<String>) {
            match n {
                dioxus_core::TemplateNode::Text { text } => out.push(text.to_string()),
                dioxus_core::TemplateNode::Element { children, .. } => {
                    for c in children.iter() {
                        walk(c, out)
                    }
                }
                _ => {}
            }
        }
        let mut v = Vec::new();
        for r in t.roots.iter() {
            walk(r, &mut v);
        }
        self.0.extend(v);
    }
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
    fn set_node_text(&mut self, t: &str, _: ElementId) {
        self.0.push(t.to_string());
    }
    fn create_event_listener(&mut self, _: &'static str, _: ElementId) {}
    fn remove_event_listener(&mut self, _: &'static str, _: ElementId) {}
    fn remove_node(&mut self, _: ElementId) {}
    fn push_root(&mut self, _: ElementId) {}
}

#[component]
fn Boom() -> Element {
    panic!("the extension's render failed");
}

#[component]
fn App() -> Element {
    let ws = use_context_provider(|| Workspace::new(config()));
    rsx! {
        div { "before" }
        {moonkale_shell::guard_extension_panel(ws, "Boom panel".into(), rsx! { Boom {} }, None, "Boom".into())}
        div { "after" }
    }
}

#[test]
fn a_panicking_panel_shows_a_notice_and_the_rest_renders() {
    let mut dom = VirtualDom::new(App);
    let mut m = Texts::default();
    dom.rebuild(&mut m);
    // The caught panic marks the boundary dirty; its fallback draws on the
    // next render.
    dom.render_immediate(&mut m);
    let all = m.0.join(" | ");
    eprintln!("written: {all}");
    assert!(all.contains("Boom panel stopped with an error"), "{all}");
    assert!(
        all.contains("the extension's render failed"),
        "the cause is shown: {all}"
    );
}

fn empty_app() -> Element {
    VNode::empty()
}

#[test]
fn a_panicking_event_disables_only_its_owner_and_suppresses_later_callbacks() {
    use moonkale_ext_api::crash::{event, ExtensionGuard};
    let dom = VirtualDom::new(empty_app);
    dom.in_scope(ScopeId::ROOT, || {
        let ws = Workspace::new(config());
        let broken = ExtensionGuard::new(ws, "broken");
        let healthy = ExtensionGuard::new(ws, "healthy");
        let mut calls = 0;
        assert!(broken
            .run::<()>(|| panic!("private payload must not be retained"))
            .is_none());
        broken.run(|| calls += 1);
        assert_eq!(calls, 0);
        assert_eq!(healthy.run(|| 42), Some(42));
        assert_eq!(ws.crashes.notice.peek().as_deref(), Some("broken"));
        assert_eq!(ws.crashes.disabled.peek().len(), 1);
        // A callback created by an extension entry point inherits ownership,
        // even when it is invoked later outside that entry point.
        let mut callback = healthy
            .run(|| event(|_: ()| -> () { panic!("event panic") }))
            .unwrap();
        callback(());
        assert!(!healthy.enabled());
    });
}

#[test]
fn a_panicking_task_after_an_await_disables_only_its_owner() {
    use moonkale_ext_api::crash::ExtensionGuard;
    let dom = VirtualDom::new(empty_app);
    dom.in_scope(ScopeId::ROOT, || {
        let ws = Workspace::new(config());
        let broken = ExtensionGuard::new(ws, "broken");
        let healthy = ExtensionGuard::new(ws, "healthy");
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(broken.task(async {
            tokio::task::yield_now().await;
            panic!("task panic after pending");
        }));
        assert!(!broken.enabled());
        assert!(healthy.enabled());
        let mut ran = false;
        rt.block_on(broken.task(async {
            ran = true;
        }));
        assert!(!ran, "sibling work must stop polling");
        assert_eq!(ws.crashes.notice.peek().as_deref(), Some("broken"));
    });
}

#[test]
fn safe_mode_ignores_saved_enablement_without_mutating_preferences() {
    let dom = VirtualDom::new(empty_app);
    dom.in_scope(ScopeId::ROOT, || {
        let mut ws = Workspace::new(config());
        let before = ws.settings.resolved.peek().extensions.clone();
        ws.crashes.safe.set(true);
        assert!(!ws
            .crashes
            .allows(&moonkale_ext_api::Manifest::core("core", "Core", "")));
        assert_eq!(ws.settings.resolved.peek().extensions, before);
    });
}

#[test]
fn async_event_callbacks_are_polled_and_guarded_instead_of_dropped() {
    use moonkale_ext_api::crash::{event, ExtensionGuard};
    let dom = VirtualDom::new(empty_app);
    dom.in_scope(ScopeId::ROOT, || {
        let ws = Workspace::new(config());
        let guard = ExtensionGuard::new(ws, "async-event");
        let mut callback = guard
            .run(|| {
                event(|_: ()| async {
                    panic!("async event panic");
                })
            })
            .unwrap();
        callback(());
        assert!(!guard.enabled());
        assert_eq!(ws.crashes.notice.peek().as_deref(), Some("async-event"));
    });
}
