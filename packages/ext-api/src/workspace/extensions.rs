//! `Workspace`: wasm extensions — list, run on the server or in the browser, answer their host calls. Split out of `workspace.rs` in Milestone 18 phase 3b
//! (no API change; phase 3c turns the areas into services).

use super::*;

impl Workspace {
    /// Re-scan installed wasm extensions (user dir + the folder's).
    pub async fn refresh_wasm_extensions(mut self) {
        let Some(w) = self.config.wasm else { return };
        let folder = self
            .sources
            .peek()
            .iter()
            .find(|s| s.descriptor.family == moonkale_core::SourceFamily::Folder)
            .and_then(|s| {
                s.descriptor
                    .id
                    .as_str()
                    .strip_prefix("folder:")
                    .map(str::to_string)
            });
        match (w.list)(folder).await {
            Ok(list) => {
                if *self.wasm_extensions.peek() != list {
                    self.wasm_extensions.set(list);
                }
            }
            Err(e) => self.set_status(format!("Extensions not scanned: {e}")),
        }
    }

    /// Run a wasm extension's command with the permissions granted in settings.
    pub async fn run_wasm_command(
        &self,
        ext_id: &str,
        command: &str,
        args: serde_json::Value,
    ) -> Result<String, String> {
        let w = self
            .config
            .wasm
            .ok_or("wasm extensions are not available on this platform")?;
        let granted = self
            .settings
            .peek()
            .extensions
            .permissions
            .get(ext_id)
            .cloned()
            .unwrap_or_default();
        // Milestone 8: in a cross-origin-isolated browser the module runs
        // here, its host calls answered by this workspace's sources; the
        // server path stays as the fallback.
        if let Some(url) = self.config.wasm_module_url {
            match self
                .run_wasm_in_browser(&url(ext_id.to_string()), command, args.clone(), &granted)
                .await
            {
                Ok(BrowserRun::Done(r)) => return r,
                Ok(BrowserRun::Unavailable) => {}
                Err(e) => return Err(e),
            }
        }
        (w.run)(ext_id.to_string(), command.to_string(), args, granted).await
    }

    /// Run a module in the page's Worker runtime (`window.moonkale.wasmHost`),
    /// answering its host calls. `Unavailable` when the page cannot (no
    /// isolation, no runtime script): the caller falls back to the server.
    pub(super) async fn run_wasm_in_browser(
        &self,
        url: &str,
        command: &str,
        args: serde_json::Value,
        granted: &[String],
    ) -> Result<BrowserRun, String> {
        use moonkale_ext_abi::{HostCall, HostReply};
        let mut ev = dioxus::document::eval(WASM_HOST_JS);
        let _ = ev.send(serde_json::json!({ "url": url, "command": command, "args": args }));
        loop {
            let msg: serde_json::Value = match ev.recv().await {
                Ok(v) => v,
                Err(e) => return Err(format!("browser runtime: {e}")),
            };
            match msg.get("kind").and_then(|k| k.as_str()) {
                Some("unavailable") => return Ok(BrowserRun::Unavailable),
                Some("log") => tracing::info!(
                    "[ext] {}",
                    msg.get("text").and_then(|t| t.as_str()).unwrap_or("")
                ),
                Some("call") => {
                    let json = msg.get("json").and_then(|j| j.as_str()).unwrap_or("");
                    let reply = match serde_json::from_str::<HostCall>(json) {
                        Ok(call) => self.answer_host_call(call, granted).await,
                        Err(e) => HostReply::err(format!("bad host call: {e}")),
                    };
                    let _ = ev.send(serde_json::to_value(reply).unwrap_or_default());
                }
                Some("done") => {
                    let reply = msg.get("reply").and_then(|r| r.as_str()).unwrap_or("");
                    let parsed: moonkale_ext_abi::RunReply = serde_json::from_str(reply)
                        .map_err(|e| format!("reply JSON: {e}: {reply}"))?;
                    return Ok(BrowserRun::Done(parsed.into_result()));
                }
                Some("error") => {
                    return Err(msg
                        .get("error")
                        .and_then(|e| e.as_str())
                        .unwrap_or("browser runtime failed")
                        .to_string())
                }
                _ => {}
            }
        }
    }

    /// The host side of the JSON ABI, over this workspace's sources, with
    /// the same permission check as the native runtime.
    pub(super) async fn answer_host_call(
        &self,
        call: moonkale_ext_abi::HostCall,
        granted: &[String],
    ) -> moonkale_ext_abi::HostReply {
        use moonkale_ext_abi::{HostCall, HostReply};
        let needed = call.permission();
        if !granted.iter().any(|g| g == needed) {
            return HostReply::err(format!(
                "permission {needed} not granted (Settings → Extensions)"
            ));
        }
        match call {
            HostCall::ListSources => {
                let list: Vec<SourceDescriptor> = self
                    .sources
                    .peek()
                    .iter()
                    .map(|s| s.descriptor.clone())
                    .collect();
                HostReply::ok(serde_json::to_value(list).unwrap_or_default())
            }
            HostCall::Query { source, query } => {
                let Some(src) = self.source(&SourceId::new(source)) else {
                    return HostReply::err("unknown source");
                };
                let query: Query = match serde_json::from_value(query) {
                    Ok(q) => q,
                    Err(e) => return HostReply::err(format!("bad query: {e}")),
                };
                match src.query(query).await {
                    Ok(res) => HostReply::ok(serde_json::to_value(res).unwrap_or_default()),
                    Err(e) => HostReply::err(e.to_string()),
                }
            }
            HostCall::FetchText { source, node } => {
                let Some(src) = self.source(&SourceId::new(source)) else {
                    return HostReply::err("unknown source");
                };
                let Ok(node) = node.parse::<NodeId>() else {
                    return HostReply::err("bad node id");
                };
                match src.fetch_text(node).await {
                    Ok((text, _)) => HostReply::ok(serde_json::Value::String(text)),
                    Err(e) => HostReply::err(e.to_string()),
                }
            }
        }
    }
}

pub(super) enum BrowserRun {
    Done(Result<String, String>),
    Unavailable,
}

/// Drives `window.moonkale.wasmHost` (packages/js/wasm-host) from Rust:
/// receives the run request, forwards host calls to Rust and back.
const WASM_HOST_JS: &str = r#"
const req = await dioxus.recv();
const host = window.moonkale && window.moonkale.wasmHost;
if (!host || !host.available()) { dioxus.send({ kind: "unavailable" }); return; }
try {
    const reply = await host.run(req.url, req.command, req.args, async (json) => {
        dioxus.send({ kind: "call", json });
        const r = await dioxus.recv();
        return JSON.stringify(r);
    }, (text) => dioxus.send({ kind: "log", text }));
    dioxus.send({ kind: "done", reply });
} catch (e) {
    dioxus.send({ kind: "error", error: String(e && e.message || e) });
}
"#;
