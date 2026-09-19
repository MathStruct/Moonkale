//! Example wasm extension for Moonkale (ABI v1, see `moonkale-ext-host`).
//!
//! Commands:
//! - `wordcount.count { source, node }` → lines / words / characters of a file,
//! - `wordcount.top { source, node, n }` → the most frequent words.
//!
//! Both read the file through the host (`fetch_text`), which requires the
//! `read-sources` permission — granted by the user in Settings → Extensions.
//!
//! Build: `cargo build -p moonkale-ext-wordcount --target wasm32-unknown-unknown --release`
//! (or `build.sh`, which also copies it into `~/.config/moonkale/extensions/`).

use moonkale_ext_host::abi::{
    HostCall, HostReply, RunReply, RunRequest, WasmCommand, WasmManifest, ABI_VERSION,
};
use serde_json::{json, Value};

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "moonkale")]
extern "C" {
    fn log(ptr: i32, len: i32);
    fn call(ptr: i32, len: i32) -> i64;
}

/// Native builds (workspace `cargo check`/`test`) get stubs.
#[cfg(not(target_arch = "wasm32"))]
unsafe fn log(_ptr: i32, _len: i32) {}
#[cfg(not(target_arch = "wasm32"))]
unsafe fn call(_ptr: i32, _len: i32) -> i64 {
    0
}

#[no_mangle]
pub extern "C" fn alloc(len: i32) -> i32 {
    let mut v: Vec<u8> = Vec::with_capacity(len.max(0) as usize);
    let ptr = v.as_mut_ptr();
    std::mem::forget(v);
    ptr as i32
}

fn pack(s: String) -> i64 {
    let len = s.len() as i64;
    let ptr = s.as_ptr() as i64;
    std::mem::forget(s);
    (ptr << 32) | len
}

fn unpack(packed: i64) -> String {
    let ptr = (packed >> 32) as usize;
    let len = (packed & 0xffff_ffff) as usize;
    if ptr == 0 {
        return String::new();
    }
    // SAFETY: the host wrote `len` bytes at `ptr` obtained from our `alloc`.
    unsafe {
        String::from_utf8_lossy(std::slice::from_raw_parts(ptr as *const u8, len)).into_owned()
    }
}

fn host(request: &HostCall) -> Result<Value, String> {
    let json = serde_json::to_string(request).unwrap();
    // SAFETY: passing a pointer/length into our own linear memory.
    let packed = unsafe { call(json.as_ptr() as i32, json.len() as i32) };
    let reply = unpack(packed);
    match serde_json::from_str::<HostReply>(&reply).map_err(|e| e.to_string())? {
        HostReply::Ok { ok: true, result } => Ok(result),
        HostReply::Ok { result, .. } => Err(result.to_string()),
        HostReply::Err { error, .. } => Err(error),
    }
}

fn info(text: &str) {
    // SAFETY: as above.
    unsafe { log(text.as_ptr() as i32, text.len() as i32) }
}

pub fn manifest_value() -> WasmManifest {
    WasmManifest {
        abi: ABI_VERSION,
        id: "dev.moonkale.example-wordcount".into(),
        name: "Word count (wasm example)".into(),
        description: "Counts lines, words and characters of a file; lists the most frequent words."
            .into(),
        permissions: vec!["read-sources".into()],
        commands: vec![
            WasmCommand {
                id: "wordcount.count".into(),
                title: "Count words".into(),
                description: "Lines, words and characters of a text file (source id + node id)."
                    .into(),
                input_schema: json!({ "type": "object", "properties": { "source": { "type": "string" }, "node": { "type": "string" } }, "required": ["source", "node"] }),
                llm_tool: true,
            },
            WasmCommand {
                id: "wordcount.top".into(),
                title: "Most frequent words".into(),
                description: "The n most frequent words of a text file.".into(),
                input_schema: json!({ "type": "object", "properties": { "source": { "type": "string" }, "node": { "type": "string" }, "n": { "type": "integer" } }, "required": ["source", "node"] }),
                llm_tool: true,
            },
        ],
    }
}

#[no_mangle]
pub extern "C" fn manifest() -> i64 {
    pack(serde_json::to_string(&manifest_value()).unwrap())
}

pub fn run_request(req: RunRequest) -> Result<String, String> {
    let source = req.args["source"]
        .as_str()
        .ok_or("source is required")?
        .to_string();
    let node = req.args["node"]
        .as_str()
        .ok_or("node is required")?
        .to_string();
    let text = host(&HostCall::FetchText { source, node })?;
    let text = text.as_str().unwrap_or_default().to_string();
    info(&format!("read {} chars", text.chars().count()));
    match req.command.as_str() {
        "wordcount.count" => Ok(format!(
            "{} lines, {} words, {} characters",
            text.lines().count(),
            text.split_whitespace().count(),
            text.chars().count()
        )),
        "wordcount.top" => {
            let n = req.args["n"].as_u64().unwrap_or(5) as usize;
            let mut counts: std::collections::HashMap<String, usize> = Default::default();
            for w in text
                .split(|c: char| !c.is_alphanumeric())
                .filter(|w| w.len() > 1)
            {
                *counts.entry(w.to_lowercase()).or_default() += 1;
            }
            let mut v: Vec<(String, usize)> = counts.into_iter().collect();
            v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            Ok(v.into_iter()
                .take(n)
                .map(|(w, c)| format!("{w}: {c}"))
                .collect::<Vec<_>>()
                .join("\n"))
        }
        other => Err(format!("unknown command {other}")),
    }
}

#[no_mangle]
pub extern "C" fn run(ptr: i32, len: i32) -> i64 {
    let input = unpack(((ptr as i64) << 32) | len as i64);
    let reply = match serde_json::from_str::<RunRequest>(&input) {
        Ok(req) => match run_request(req) {
            Ok(result) => RunReply::Ok { ok: true, result },
            Err(error) => RunReply::Err { ok: false, error },
        },
        Err(e) => RunReply::Err {
            ok: false,
            error: format!("bad request: {e}"),
        },
    };
    pack(serde_json::to_string(&reply).unwrap())
}
