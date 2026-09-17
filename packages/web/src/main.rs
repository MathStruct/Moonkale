//! Web entrypoint. The folder lives on the server: `RemoteSource` over the
//! `api` server functions. No native window, so no `WindowControls`. Tabs
//! and windows of the same origin form one session over `BroadcastChannel`.

use dioxus::prelude::*;
use moonkale_core::{Source, SourceDescriptor};
use std::rc::Rc;
use std::sync::Arc;
use ui::{
    Frame, OpenFolderFuture, SessionBus, SessionMessage, Shell, ShellConfig, WorkspaceConfig,
};

const FAVICON: Asset = asset!("/assets/favicon.ico");
const MAIN_CSS: Asset = asset!("/assets/main.css");

fn main() {
    dioxus::launch(App);
}

fn open_remote(path: String) -> OpenFolderFuture {
    Box::pin(async move {
        api::RemoteSource::open_folder(&path)
            .await
            .map(|s| Arc::new(s) as Arc<dyn Source>)
    })
}

/// Another tab already opened this source on the server: just wrap its descriptor.
fn attach_remote(descriptor: SourceDescriptor) -> OpenFolderFuture {
    Box::pin(async move {
        Ok(Arc::new(api::RemoteSource::from_descriptor(descriptor)) as Arc<dyn Source>)
    })
}

/// Session bus over `BroadcastChannel`: every tab/window of this origin in
/// the same browser hears every message. One eval carries both directions.
struct BroadcastBus {
    eval: dioxus::document::Eval,
}

impl SessionBus for BroadcastBus {
    fn send(&self, msg: SessionMessage) {
        let _ = self.eval.send(msg);
    }
}

fn session(deliver: Callback<SessionMessage>) -> Rc<dyn SessionBus> {
    let eval = document::eval(
        r#"
        const bc = new BroadcastChannel("moonkale-session");
        bc.onmessage = (e) => dioxus.send(e.data);
        for (;;) { bc.postMessage(await dioxus.recv()); }
        "#,
    );
    let mut rx = eval;
    spawn(async move {
        loop {
            match rx.recv::<SessionMessage>().await {
                Ok(msg) => deliver.call(msg),
                // A peer sent something we can't read (version skew): skip it.
                Err(dioxus::document::EvalError::Serialization(e)) => {
                    tracing::warn!("session: bad message: {e}")
                }
                Err(_) => break,
            }
        }
    });
    Rc::new(BroadcastBus { eval })
}

fn new_window() {
    // A *window*, not a tab: a background tab can never be a drop target.
    document::eval("window.open(location.href, '_blank', 'popup,width=1200,height=800');");
}

#[component]
fn App() -> Element {
    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        Frame {
            config: ShellConfig {
                extensions: ui::default_extensions,
                workspace: WorkspaceConfig { open_folder: open_remote, pick_folder: None, attach_source: attach_remote },
                session,
                new_window: Some(new_window),
            },
            Shell {}
        }
    }
}
