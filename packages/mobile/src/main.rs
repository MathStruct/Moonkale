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
    // Milestone 12: server functions go to a loopback relay of our own
    // (dioxus's server URL is set once, P-098); it pipes to the server the
    // user connects to with *File → Connect to Server…*.
    match api::relay::install() {
        Ok(url) => dioxus::fullstack::set_server_url(url.leak()),
        Err(e) => {
            tracing::warn!("mobile: client relay not started ({e}); Connect to Server is off")
        }
    }
    dioxus::launch(App);
}

/// Sources: the phone's own folder, or the connected server's.
fn open_any(path: String, options: ui::OpenOptions) -> OpenFolderFuture {
    if api::client::active().is_some() {
        return api::client::open_folder(path, options);
    }
    open_local(path, options)
}

fn attach_any(descriptor: SourceDescriptor) -> AttachFuture {
    if api::client::active().is_some() {
        return api::client::attach_source(descriptor);
    }
    attach_local(descriptor)
}

/// A shell on the connected server (the phone has none of its own).
fn spawn_terminal(
    cwd: Option<String>,
    cols: u16,
    rows: u16,
) -> moonkale_terminal::SpawnTerminalFuture {
    if api::client::active().is_some() {
        return api::client::spawn_terminal(cwd, cols, rows);
    }
    Box::pin(async {
        Err(
            "no terminal on the phone — connect to a server (File → Connect to Server…)"
                .to_string(),
        )
    })
}

fn git_any(
    root: String,
    req: moonkale_ext_git::GitRequest,
) -> ui::SettingsFuture<moonkale_ext_git::GitResponse> {
    if api::client::active().is_some() {
        return moonkale_ext_git::remote(root, req);
    }
    Box::pin(async { Err("git needs a server on the phone".to_string()) })
}

fn server_client() -> ui::ServerClient {
    ui::ServerClient {
        connect: |url, token| {
            api::client::connect(&url, token.as_deref(), &url);
            Ok(())
        },
        disconnect: api::client::disconnect,
        active: || api::client::active().map(|r| r.label),
    }
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
                extensions: moonkale_distribution::default_extensions,
                workspace: workspace_config(),
                session,
                new_window: None,
            },
            Shell {}
        }
    }
}

/// The source openers of this app: every driver crate's (Milestone 18
/// phase 2). Moves to the `distribution` crate in phase 4.
fn openers() -> &'static moonkale_core::Openers {
    static OPENERS: std::sync::OnceLock<moonkale_core::Openers> = std::sync::OnceLock::new();
    OPENERS.get_or_init(|| {
        moonkale_core::Openers::new(
            [
                moonkale_sources_sql::openers(),
                moonkale_sources_kv::openers(),
                moonkale_sources_graph::openers(),
            ]
            .concat(),
        )
    })
}

/// This phone's state store (ADR-0014): `state.sqlite` next to settings.json.
fn state_access() -> Option<ui::StateAccess> {
    let path = settings_path().with_file_name("state.sqlite");
    match moonkale_state_stores::SqliteStore::open_with(&path, moonkale_state::Durability::Relaxed)
    {
        Ok(store) => Some(ui::local_state(std::sync::Arc::new(store))),
        Err(e) => {
            tracing::warn!("state: no store ({e}); layouts stay in the folder");
            None
        }
    }
}

/// What this platform gives the workspace (Milestone 18 phase 3c: grouped by
/// what each part needs from the platform).
/// Services the extensions define (Milestone 18 phase 4.2).
static GIT: moonkale_ext_git::GitRunner = moonkale_ext_git::GitRunner(git_any);
static SERVICES: [&(dyn std::any::Any + Sync); 1] = [&GIT];

fn workspace_config() -> WorkspaceConfig {
    let state = state_access();
    WorkspaceConfig {
        folders: ui::FolderAccess {
            open: open_any,
            pick: None,
            attach: attach_any,
            reopen_last: true,
            openers: openers(),
        },
        processes: ui::Processes {
            terminal: Some(spawn_terminal),
            ..Default::default()
        },
        persistence: ui::Persistence {
            settings: Some(ui::SettingsStore {
                load: load_settings,
                save: save_settings,
            }),
            state,
            // The folder's host keeps its entity log: this store for local
            // folders, the server's while connected (phase 5.10).
            host: state.map(|_| api::client::host_state_routed()),
            // The user's settings too (phase 5.18): `settings.json` is
            // imported once, then only the store is written.
            user_settings_in_state: true,
            ..Default::default()
        },
        network: ui::Network {
            agent_sessions: Some(api::client::agent_sessions(api::client::agent_available)),
            server: Some(server_client()),
            ..Default::default()
        },
        runtimes: Default::default(),
        services: &SERVICES,
    }
}
