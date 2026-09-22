//! Web entrypoint. The folder lives on the server: `RemoteSource` over the
//! `api` server functions. No native window, so no `WindowControls`. Tabs
//! and windows of the same origin form one session over `BroadcastChannel`.

use dioxus::prelude::*;
use moonkale_core::{Source, SourceDescriptor};
use std::rc::Rc;
use std::sync::Arc;
use ui::{
    AttachFuture, Frame, OpenFolderFuture, SessionBus, SessionMessage, Shell, ShellConfig,
    WorkspaceConfig,
};

const FAVICON: Asset = asset!("/assets/favicon.ico");
const ICON_PNG: Asset = asset!("/assets/icon.png");
const MAIN_CSS: Asset = asset!("/assets/main.css");

fn main() {
    // The server adds the MCP endpoint next to the app's own routes.
    #[cfg(feature = "server")]
    {
        // Standalone server flags (Milestone 11): what an SSH-launched or a
        // hand-started `moonkale-server` is told on its command line.
        //   --port N        listen port (also PORT)
        //   --bind IP       listen address (also IP; loopback unless a token is set)
        //   --root PATH     the folder jail (also MOONKALE_ROOT)
        //   --token-stdin   read the access token from the first line of stdin
        //   --version       print the version and exit
        let mut args = std::env::args().skip(1);
        while let Some(a) = args.next() {
            match a.as_str() {
                "--port" => {
                    if let Some(v) = args.next() {
                        std::env::set_var("PORT", v);
                    }
                }
                "--bind" => {
                    if let Some(v) = args.next() {
                        std::env::set_var("IP", v);
                    }
                }
                "--root" => {
                    if let Some(v) = args.next() {
                        std::env::set_var("MOONKALE_ROOT", v);
                    }
                }
                "--token-stdin" => {
                    let mut line = String::new();
                    let _ = std::io::stdin().read_line(&mut line);
                    let t = line.trim().to_string();
                    if t.is_empty() {
                        eprintln!("moonkale: --token-stdin: no token on stdin");
                        std::process::exit(2);
                    }
                    std::env::set_var("MOONKALE_TOKEN", t);
                }
                "--version" => {
                    println!("moonkale-server {}", env!("CARGO_PKG_VERSION"));
                    return;
                }
                other => {
                    eprintln!("moonkale: unknown argument {other}");
                    std::process::exit(2);
                }
            }
        }
        // Refuse a non-loopback bind without MOONKALE_TOKEN (Milestone 7)
        // or without TLS (Milestone 11).
        api::auth::guard_bind();
        fn build_router() -> axum::Router {
            let router = dioxus::server::router(App)
                .route("/mcp", axum::routing::post(api::mcp::handler))
                .route(
                    "/api/ext/module/{id}",
                    axum::routing::get(api::module_bytes),
                );
            api::auth::protect(router)
        }
        if let Some((cert, key)) = api::auth::tls_files() {
            serve_tls(cert, key, build_router);
        }
        dioxus::server::serve(|| async { Ok(build_router()) });
    }
    #[cfg(not(feature = "server"))]
    dioxus::launch(App);
}

/// HTTPS with the PEM files in `MOONKALE_TLS_CERT`/`MOONKALE_TLS_KEY`
/// (Milestone 11): the same router, served by `axum-server` + rustls on
/// the `IP`/`PORT` address. No hot reload here — this is the deployed path.
#[cfg(feature = "server")]
fn serve_tls(cert: String, key: String, build: fn() -> axum::Router) -> ! {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let addr = dioxus::cli_config::fullstack_address_or_localhost();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    rt.block_on(async move {
        let config = axum_server::tls_rustls::RustlsConfig::from_pem_file(&cert, &key)
            .await
            .unwrap_or_else(|e| {
                eprintln!("moonkale: TLS: cannot load {cert} / {key}: {e}");
                std::process::exit(2);
            });
        eprintln!("moonkale: serving https://{addr}");
        axum_server::bind_rustls(addr, config)
            .serve(build().into_make_service())
            .await
            .expect("https server");
    });
    std::process::exit(0)
}

fn open_remote(path: String, options: ui::OpenOptions) -> OpenFolderFuture {
    Box::pin(async move {
        api::RemoteSource::open_folder(&path, options.embed)
            .await
            .map(|v| {
                v.into_iter()
                    .map(|s| Arc::new(s) as Arc<dyn Source>)
                    .collect()
            })
    })
}

/// Another tab already opened this source on the server: just wrap its descriptor.
fn attach_remote(descriptor: SourceDescriptor) -> AttachFuture {
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

/// Web terminals run on the server (dev-server feature; see api::terminal).
fn spawn_terminal(cwd: Option<String>, cols: u16, rows: u16) -> ui::SpawnTerminalFuture {
    Box::pin(async move {
        api::RemoteTerminal::connect(cwd, cols, rows)
            .await
            .map(|t| Box::new(t) as Box<dyn ui::TerminalBackend>)
    })
}

fn compile_typst(root: String, main_rel: String, text: String) -> ui::CompileTypstFuture {
    Box::pin(async move {
        api::compile_typst(root, main_rel, text)
            .await
            .unwrap_or_else(|e| Err(vec![e.to_string()]))
    })
}

/// Language servers run on the server; the client sees a websocket.
fn spawn_lsp(language: String, root: String) -> ui::LspTransportFuture {
    Box::pin(async move {
        api::RemoteLsp::connect(language, root)
            .await
            .map(|t| Box::new(t) as Box<dyn ui::LspTransport>)
    })
}

/// The provider lives on the server; the client talks to `/api/llm`.
#[cfg(target_arch = "wasm32")]
fn llm_provider(settings: moonkale_llm::LlmSettings) -> ui::LlmProviderFuture {
    Box::pin(async move {
        api::RemoteProvider::connect(settings)
            .await
            .map(|p| std::sync::Arc::new(p) as std::sync::Arc<dyn moonkale_llm::Provider>)
    })
}
#[cfg(not(target_arch = "wasm32"))]
fn llm_provider(_settings: moonkale_llm::LlmSettings) -> ui::LlmProviderFuture {
    // Server-side render only: the real provider is connected on the client.
    Box::pin(async move { Err("no provider during server render".into()) })
}

/// User settings live in the browser (`localStorage`); workspace settings
/// go through the folder on the server like any file.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
const SETTINGS_KEY: &str = "moonkale.settings";

#[cfg(target_arch = "wasm32")]
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

fn load_settings() -> ui::SettingsFuture<ui::SettingsFile> {
    Box::pin(async move {
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(s) = local_storage() {
                if let Ok(Some(text)) = s.get_item(SETTINGS_KEY) {
                    return ui::SettingsFile::parse(&text);
                }
            }
        }
        Ok(ui::SettingsFile::new())
    })
}

fn save_settings(file: ui::SettingsFile) -> ui::SettingsFuture<()> {
    Box::pin(async move {
        #[cfg(target_arch = "wasm32")]
        {
            let s = local_storage().ok_or("localStorage unavailable")?;
            return s
                .set_item(SETTINGS_KEY, &file.to_json())
                .map_err(|_| "localStorage write failed".to_string());
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = file;
            Ok(())
        }
    })
}

/// wasm extensions run on the server; the client lists and calls.
fn join_presence(
    room: String,
    member: ui::PresenceMember,
    on_members: Callback<Vec<ui::PresenceMember>>,
) -> Rc<dyn ui::PresenceLink> {
    Rc::new(api::presence::RemotePresence::join(
        room, member, on_members,
    ))
}
fn git_remote(root: String, req: ui::GitRequest) -> ui::SettingsFuture<ui::GitResponse> {
    Box::pin(async move {
        match api::git_run(root, req).await {
            Ok(r) => r,
            Err(e) => Err(e.to_string()),
        }
    })
}
fn wasm_list(folder: Option<String>) -> ui::SettingsFuture<Vec<moonkale_ext_host::WasmManifest>> {
    Box::pin(async move {
        api::list_wasm_extensions(folder)
            .await
            .map_err(|e| e.to_string())
    })
}
fn wasm_run(
    ext: String,
    command: String,
    args: serde_json::Value,
    granted: Vec<String>,
) -> ui::SettingsFuture<String> {
    Box::pin(async move {
        api::run_wasm_command(ext, command, args, granted)
            .await
            .map_err(|e| e.to_string())?
    })
}

fn new_window() {
    // A *window*, not a tab: a background tab can never be a drop target.
    document::eval("window.open(location.href, '_blank', 'popup,width=1200,height=800');");
}

#[component]
fn App() -> Element {
    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "apple-touch-icon", href: ICON_PNG }
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        Frame {
            config: ShellConfig {
                extensions: ui::default_extensions,
                workspace: WorkspaceConfig { open_folder: open_remote, pick_folder: None, attach_source: attach_remote, spawn_terminal: Some(spawn_terminal), compile_typst: Some(compile_typst), spawn_lsp: Some(spawn_lsp), llm: Some(llm_provider), settings_store: Some(ui::SettingsStore { load: load_settings, save: save_settings }), secret_store: None, reopen_last_folder: false, wasm: Some(ui::WasmExtensions { list: wasm_list, run: wasm_run }), git: Some(git_remote), presence: Some(join_presence), wasm_module_url: Some(|id| format!("/api/ext/module/{id}")), remote: None, agent_sessions: Some(api::client::agent_sessions(|| true)), server: None, spawn_program: None },
                session,
                new_window: Some(new_window),
            },
            Shell {}
        }
    }
}
