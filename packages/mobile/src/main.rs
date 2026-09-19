//! Mobile entrypoint. Reads folders in-process (the app sandbox until the
//! Storage Access Framework lands — see markdown/packaging/Android.md); no
//! native dialog yet.

use dioxus::prelude::*;
use moonkale_core::{Source, SourceDescriptor, SourceError};
use std::rc::Rc;
use std::sync::Arc;
use ui::{
    AttachFuture, Frame, OpenFolderFuture, SessionBus, SessionMessage, Shell, ShellConfig,
    WorkspaceConfig,
};

const MAIN_CSS: Asset = asset!("/assets/main.css");

fn main() {
    dioxus::launch(App);
}

fn open_local(path: String, _options: ui::OpenOptions) -> OpenFolderFuture {
    Box::pin(async move {
        let path = if path.trim().is_empty() {
            ".".to_string()
        } else {
            path
        };
        moonkale_project_fs::FolderSource::open(&path)
            .map(|s| vec![Arc::new(s) as Arc<dyn Source>])
            .map_err(SourceError::from)
    })
}

fn attach_local(descriptor: SourceDescriptor) -> AttachFuture {
    let path = descriptor
        .id
        .as_str()
        .strip_prefix("folder:")
        .unwrap_or(".")
        .to_string();
    Box::pin(async move {
        open_local(path, ui::OpenOptions::default())
            .await?
            .into_iter()
            .next()
            .ok_or(SourceError::NotFound)
    })
}

/// One window on mobile: a bus with nobody to talk to.
struct NoBus;
impl SessionBus for NoBus {
    fn send(&self, _msg: SessionMessage) {}
}
fn session(_deliver: Callback<SessionMessage>) -> Rc<dyn SessionBus> {
    Rc::new(NoBus)
}

#[component]
fn App() -> Element {
    rsx! {
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        Frame {
            config: ShellConfig {
                extensions: ui::default_extensions,
                workspace: WorkspaceConfig { open_folder: open_local, pick_folder: None, attach_source: attach_local, spawn_terminal: None, compile_typst: None, spawn_lsp: None, llm: None, settings_store: None, secret_store: None, reopen_last_folder: false, wasm: None, git: None },
                session,
                new_window: None,
            },
            Shell {}
        }
    }
}
