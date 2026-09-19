//! wasmtime runtime for ABI v1 core modules (native only).

use crate::abi::{HostCall, HostReply, RunReply, RunRequest, WasmManifest, ABI_VERSION};
use moonkale_core::{NodeId, Query, Source, SourceDescriptor, SourceId};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use wasmtime::{Caller, Engine, Extern, Linker, Memory, Module, Store, TypedFunc};

/// What the host offers to extensions (implemented over a source registry).
pub trait Host: Send + Sync {
    fn list_sources(&self) -> Vec<SourceDescriptor>;
    fn source(&self, id: &SourceId) -> Option<Arc<dyn Source>>;
}

/// A loaded module with its manifest; instantiated per call (cheap for
/// small modules, and every run starts from a clean state).
pub struct LoadedExtension {
    pub path: PathBuf,
    pub manifest: WasmManifest,
    module: Module,
}

pub struct Runtime {
    engine: Engine,
    pub extensions: Vec<LoadedExtension>,
}

struct State {
    host: Arc<dyn Host>,
    granted: Vec<String>,
    handle: tokio::runtime::Handle,
    log_prefix: String,
}

impl Runtime {
    pub fn new() -> Result<Self, String> {
        let engine = Engine::default();
        Ok(Self {
            engine,
            extensions: Vec::new(),
        })
    }

    /// Load one `.wasm`; reads its manifest right away.
    pub fn load(&mut self, path: &Path) -> Result<&LoadedExtension, String> {
        let module = Module::from_file(&self.engine, path)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let manifest = self.read_manifest(&module)?;
        if manifest.abi != ABI_VERSION {
            return Err(format!(
                "{}: ABI {} (host speaks {ABI_VERSION})",
                path.display(),
                manifest.abi
            ));
        }
        if manifest.id.is_empty() {
            return Err(format!("{}: manifest has no id", path.display()));
        }
        self.extensions.retain(|e| e.manifest.id != manifest.id);
        self.extensions.push(LoadedExtension {
            path: path.to_path_buf(),
            manifest,
            module,
        });
        Ok(self.extensions.last().unwrap())
    }

    fn read_manifest(&self, module: &Module) -> Result<WasmManifest, String> {
        let mut store = Store::new(&self.engine, ());
        let mut linker: Linker<()> = Linker::new(&self.engine);
        // The manifest must not need host calls: stub them.
        linker
            .func_wrap(
                "moonkale",
                "log",
                |_caller: Caller<'_, ()>, _p: i32, _l: i32| {},
            )
            .map_err(|e| e.to_string())?;
        linker
            .func_wrap(
                "moonkale",
                "call",
                |_caller: Caller<'_, ()>, _p: i32, _l: i32| -> i64 { 0 },
            )
            .map_err(|e| e.to_string())?;
        let instance = linker
            .instantiate(&mut store, module)
            .map_err(|e| e.to_string())?;
        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or("module exports no memory")?;
        let manifest: TypedFunc<(), i64> = instance
            .get_typed_func(&mut store, "manifest")
            .map_err(|e| format!("manifest export: {e}"))?;
        let packed = manifest.call(&mut store, ()).map_err(|e| e.to_string())?;
        let json = read_packed(&memory, &store, packed)?;
        serde_json::from_str(&json).map_err(|e| format!("manifest JSON: {e}"))
    }

    /// Run a command with the given granted permissions. Blocking: call it
    /// from `spawn_blocking` (host calls block on the given runtime handle).
    pub fn run(
        &self,
        ext_id: &str,
        command: &str,
        args: serde_json::Value,
        granted: Vec<String>,
        host: Arc<dyn Host>,
        handle: tokio::runtime::Handle,
    ) -> Result<String, String> {
        let ext = self
            .extensions
            .iter()
            .find(|e| e.manifest.id == ext_id)
            .ok_or_else(|| format!("unknown extension {ext_id}"))?;
        if !ext.manifest.commands.iter().any(|c| c.id == command) {
            return Err(format!("{ext_id} has no command {command}"));
        }
        let mut store = Store::new(
            &self.engine,
            State {
                host,
                granted,
                handle,
                log_prefix: ext.manifest.id.clone(),
            },
        );
        let mut linker: Linker<State> = Linker::new(&self.engine);
        linker
            .func_wrap(
                "moonkale",
                "log",
                |mut caller: Caller<'_, State>, ptr: i32, len: i32| {
                    if let Some(memory) = memory_of(&mut caller) {
                        if let Ok(text) = read(&memory, &caller, ptr, len) {
                            eprintln!("[ext {}] {text}", caller.data().log_prefix);
                        }
                    }
                },
            )
            .map_err(|e| e.to_string())?;
        linker
            .func_wrap(
                "moonkale",
                "call",
                |mut caller: Caller<'_, State>, ptr: i32, len: i32| -> i64 {
                    let Some(memory) = memory_of(&mut caller) else {
                        return 0;
                    };
                    let reply = match read(&memory, &caller, ptr, len)
                        .map_err(HostReply::err)
                        .and_then(|json| {
                            serde_json::from_str::<HostCall>(&json)
                                .map_err(|e| HostReply::err(format!("bad host call: {e}")))
                        }) {
                        Ok(call) => handle_call(caller.data(), call),
                        Err(reply) => reply,
                    };
                    let json = serde_json::to_string(&reply)
                        .unwrap_or_else(|_| "{\"ok\":false,\"error\":\"encode\"}".into());
                    write_packed(&mut caller, &memory, &json).unwrap_or(0)
                },
            )
            .map_err(|e| e.to_string())?;
        let instance = linker
            .instantiate(&mut store, &ext.module)
            .map_err(|e| e.to_string())?;
        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or("module exports no memory")?;
        let request = serde_json::to_string(&RunRequest {
            command: command.to_string(),
            args,
        })
        .unwrap();
        let alloc: TypedFunc<i32, i32> = instance
            .get_typed_func(&mut store, "alloc")
            .map_err(|e| format!("alloc export: {e}"))?;
        let ptr = alloc
            .call(&mut store, request.len() as i32)
            .map_err(|e| e.to_string())?;
        memory
            .write(&mut store, ptr as usize, request.as_bytes())
            .map_err(|e| e.to_string())?;
        let run: TypedFunc<(i32, i32), i64> = instance
            .get_typed_func(&mut store, "run")
            .map_err(|e| format!("run export: {e}"))?;
        let packed = run
            .call(&mut store, (ptr, request.len() as i32))
            .map_err(|e| format!("extension trapped: {e}"))?;
        let json = read_packed(&memory, &store, packed)?;
        let reply: RunReply =
            serde_json::from_str(&json).map_err(|e| format!("reply JSON: {e}: {json}"))?;
        reply.into_result()
    }
}

fn handle_call(state: &State, call: HostCall) -> HostReply {
    let needed = call.permission();
    if !state.granted.iter().any(|g| g == needed) {
        return HostReply::err(format!(
            "permission {needed} not granted (Settings → Extensions)"
        ));
    }
    match call {
        HostCall::ListSources => {
            HostReply::ok(serde_json::to_value(state.host.list_sources()).unwrap_or_default())
        }
        HostCall::Query { source, query } => {
            let Some(src) = state.host.source(&SourceId::new(source)) else {
                return HostReply::err("unknown source");
            };
            let query: Query = match serde_json::from_value(query) {
                Ok(q) => q,
                Err(e) => return HostReply::err(format!("bad query: {e}")),
            };
            match state.handle.block_on(src.query(query)) {
                Ok(res) => HostReply::ok(serde_json::to_value(res).unwrap_or_default()),
                Err(e) => HostReply::err(e.to_string()),
            }
        }
        HostCall::FetchText { source, node } => {
            let Some(src) = state.host.source(&SourceId::new(source)) else {
                return HostReply::err("unknown source");
            };
            let Ok(node) = node.parse::<NodeId>() else {
                return HostReply::err("bad node id");
            };
            match state.handle.block_on(src.fetch_text(node)) {
                Ok((text, _)) => HostReply::ok(serde_json::Value::String(text)),
                Err(e) => HostReply::err(e.to_string()),
            }
        }
    }
}

fn memory_of(caller: &mut Caller<'_, State>) -> Option<Memory> {
    match caller.get_export("memory") {
        Some(Extern::Memory(m)) => Some(m),
        _ => None,
    }
}

fn read(
    memory: &Memory,
    store: impl wasmtime::AsContext,
    ptr: i32,
    len: i32,
) -> Result<String, String> {
    let mut buf = vec![0u8; len.max(0) as usize];
    memory
        .read(store, ptr as usize, &mut buf)
        .map_err(|e| e.to_string())?;
    String::from_utf8(buf).map_err(|e| e.to_string())
}

fn read_packed(
    memory: &Memory,
    store: impl wasmtime::AsContext,
    packed: i64,
) -> Result<String, String> {
    let ptr = (packed >> 32) as i32;
    let len = (packed & 0xffff_ffff) as i32;
    read(memory, store, ptr, len)
}

fn write_packed(
    caller: &mut Caller<'_, State>,
    memory: &Memory,
    text: &str,
) -> Result<i64, String> {
    let alloc = match caller.get_export("alloc") {
        Some(Extern::Func(f)) => f.typed::<i32, i32>(&*caller).map_err(|e| e.to_string())?,
        _ => return Err("no alloc".into()),
    };
    let ptr = alloc
        .call(&mut *caller, text.len() as i32)
        .map_err(|e| e.to_string())?;
    memory
        .write(&mut *caller, ptr as usize, text.as_bytes())
        .map_err(|e| e.to_string())?;
    Ok(((ptr as i64) << 32) | (text.len() as i64))
}

/// `*.wasm` files in the user's extension directory and the folder's
/// `.moonkale/extensions`.
pub fn discover(config_dir: Option<&Path>, folder: Option<&Path>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let dirs = [
        config_dir.map(|d| d.join("extensions")),
        folder.map(|f| f.join(".moonkale/extensions")),
    ];
    for dir in dirs.into_iter().flatten() {
        if let Ok(rd) = std::fs::read_dir(&dir) {
            let mut files: Vec<PathBuf> = rd
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "wasm"))
                .collect();
            files.sort();
            out.extend(files);
        }
    }
    out
}
