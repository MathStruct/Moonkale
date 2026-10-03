//! wasm extensions on the server (Milestone 6): the runtime lives here; the
//! web client only lists manifests and asks for commands to run. Same
//! dev-server caveats as everything else on this server (no auth).

use dioxus::prelude::*;
use moonkale_ext_host::WasmManifest;
use serde_json::Value;

#[cfg(feature = "server")]
mod native {
    use super::*;
    use moonkale_core::{Source, SourceDescriptor, SourceId};
    use moonkale_ext_host::{discover, Host, Runtime};
    use std::sync::{Arc, Mutex, OnceLock};

    struct RegistryHost;
    impl Host for RegistryHost {
        fn list_sources(&self) -> Vec<SourceDescriptor> {
            crate::state::registry().descriptors()
        }
        fn source(&self, id: &SourceId) -> Option<Arc<dyn Source>> {
            crate::state::registry().get(id)
        }
    }

    pub fn runtime() -> &'static Mutex<Runtime> {
        static RT: OnceLock<Mutex<Runtime>> = OnceLock::new();
        RT.get_or_init(|| Mutex::new(Runtime::new().expect("wasmtime engine")))
    }

    /// Load (or reload) every discovered module; returns the manifests.
    pub fn scan(folder: Option<String>) -> Vec<WasmManifest> {
        let config = moonkale_llm::secrets::config_dir();
        let folder = folder
            .and_then(|p| crate::state::jail_dir(Some(&p)).ok())
            .map(std::path::PathBuf::from);
        let paths = discover(config.as_deref(), folder.as_deref());
        let mut rt = runtime().lock().unwrap();
        for p in &paths {
            if let Err(e) = rt.load(p) {
                eprintln!("moonkale: extension {}: {e}", p.display());
            }
        }
        rt.extensions.iter().map(|e| e.manifest.clone()).collect()
    }

    pub async fn run(
        ext: String,
        command: String,
        args: Value,
        granted: Vec<String>,
    ) -> Result<String, String> {
        let handle = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || {
            let rt = runtime().lock().unwrap();
            rt.run(
                &ext,
                &command,
                args,
                granted,
                Arc::new(RegistryHost),
                handle,
            )
        })
        .await
        .map_err(|e| e.to_string())?
    }
}

/// `GET /api/ext/module/{id}` (Milestone 8): the module's bytes for the
/// browser runtime. Plain axum route (server functions would JSON-encode
/// the bytes); mounted by `web/src/main.rs`, behind the auth gate.
#[cfg(feature = "server")]
pub async fn module_bytes(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let path = {
        let rt = native::runtime().lock().unwrap();
        rt.extensions
            .iter()
            .find(|e| e.manifest.id == id)
            .map(|e| e.path.clone())
    };
    match path.and_then(|p| std::fs::read(p).ok()) {
        Some(bytes) => (
            [(axum::http::header::CONTENT_TYPE, "application/wasm")],
            bytes,
        )
            .into_response(),
        None => (axum::http::StatusCode::NOT_FOUND, "no such extension").into_response(),
    }
}

/// Manifests of the installed extensions (server's config dir + the folder).
#[post("/api/ext/list")]
pub async fn list_wasm_extensions(
    folder: Option<String>,
) -> Result<Vec<WasmManifest>, ServerFnError> {
    Ok(native::scan(folder))
}

/// Run one command with the client's granted permissions (the client's
/// settings decide; the server trusts its own dev user).
#[post("/api/ext/run")]
pub async fn run_wasm_command(
    ext: String,
    command: String,
    args: Value,
    granted: Vec<String>,
) -> Result<Result<String, String>, ServerFnError> {
    Ok(native::run(ext, command, args, granted).await)
}
