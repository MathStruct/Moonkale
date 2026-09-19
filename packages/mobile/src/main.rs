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

/// The app's private files directory (Milestone 9, Android): the folder a
/// blank path opens. Seeded with a small vault on first launch so there is
/// something to browse before the Storage Access Framework lands.
fn app_folder() -> std::path::PathBuf {
    let candidates = [
        std::env::var("MOONKALE_ROOT")
            .ok()
            .map(std::path::PathBuf::from),
        Some(std::path::PathBuf::from(
            "/data/data/io.github.mathstruct.moonkale/files",
        )),
        std::env::current_dir().ok(),
    ];
    let dir = candidates
        .into_iter()
        .flatten()
        .find(|p| p.exists() || std::fs::create_dir_all(p).is_ok())
        .unwrap_or_else(|| std::path::PathBuf::from("/"));
    let vault = dir.join("vault");
    if !vault.exists() && std::fs::create_dir_all(vault.join("notes")).is_ok() {
        let _ = std::fs::write(vault.join("Home.md"), "# Home\n\nWelcome to Moonkale on this phone. See [[notes/First note]] and [[Ideas]].\n");
        let _ = std::fs::write(
            vault.join("notes/First note.md"),
            "Back to [[Home]]. Everything you open becomes a graph.\n",
        );
        let _ = std::fs::write(
            vault.join("Ideas.md"),
            "- open a database\n- draw the graph\n- ask the agent\n",
        );
        let _ = std::fs::write(
            vault.join("main.rs"),
            "fn main() {\n    println!(\"hello from a phone\");\n}\n",
        );
    }
    if vault.exists() {
        vault
    } else {
        dir
    }
}

/// User settings next to the vault (`<files dir>/settings.json`).
fn settings_path() -> std::path::PathBuf {
    let dir = app_folder();
    dir.parent()
        .map(|p| p.to_path_buf())
        .unwrap_or(dir)
        .join("settings.json")
}

fn load_settings() -> ui::SettingsFuture<ui::SettingsFile> {
    Box::pin(async move {
        match std::fs::read_to_string(settings_path()) {
            Ok(text) => ui::SettingsFile::parse(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ui::SettingsFile::new()),
            Err(e) => Err(e.to_string()),
        }
    })
}

fn save_settings(file: ui::SettingsFile) -> ui::SettingsFuture<()> {
    Box::pin(
        async move { std::fs::write(settings_path(), file.to_json()).map_err(|e| e.to_string()) },
    )
}

fn open_local(path: String, _options: ui::OpenOptions) -> OpenFolderFuture {
    Box::pin(async move {
        let path = if path.trim().is_empty() {
            app_folder().to_string_lossy().into_owned()
        } else {
            path
        };
        tracing::info!("mobile: opening {path}");
        let folder: Arc<dyn Source> =
            Arc::new(moonkale_project_fs::FolderSource::open(&path).map_err(SourceError::from)?);
        // The index (links, symbols, search) as on desktop; no embeddings here.
        let index: Arc<dyn Source> =
            Arc::new(moonkale_index::IndexSource::build(folder.clone()).await?);
        Ok(vec![folder, index])
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
                workspace: WorkspaceConfig { open_folder: open_local, pick_folder: None, attach_source: attach_local, spawn_terminal: None, compile_typst: None, spawn_lsp: None, llm: None, settings_store: Some(ui::SettingsStore { load: load_settings, save: save_settings }), secret_store: None, reopen_last_folder: true, wasm: None, git: None, presence: None, wasm_module_url: None },
                session,
                new_window: None,
            },
            Shell {}
        }
    }
}
