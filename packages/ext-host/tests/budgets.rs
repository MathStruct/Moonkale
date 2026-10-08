//! Audit #4: a module from an opened folder cannot wedge or flood the host,
//! take a built-in id, or use permissions it did not declare.
#![cfg(feature = "wasmtime")]

use moonkale_core::{Source, SourceDescriptor, SourceId};
use moonkale_ext_host::{Host, Runtime};
use std::sync::Arc;

struct NoSources;
impl Host for NoSources {
    fn list_sources(&self) -> Vec<SourceDescriptor> {
        Vec::new()
    }
    fn source(&self, _: &SourceId) -> Option<Arc<dyn Source>> {
        None
    }
}

/// A module with a bump allocator, `manifest` returning `json` (at offset 0;
/// or `manifest_body` instead), `extra` bytes at offset 2048, and `run`
/// executing `run_body`.
fn module(
    dir: &std::path::Path,
    name: &str,
    json: &str,
    manifest_body: Option<&str>,
    extra: &str,
    run_body: &str,
) -> std::path::PathBuf {
    let quote = |t: &str| t.replace('\\', "\\\\").replace('"', "\\\"");
    let (escaped, extra) = (quote(json), quote(extra));
    let manifest = manifest_body
        .map(str::to_string)
        .unwrap_or_else(|| format!("(i64.const {})", json.len()));
    let wat = format!(
        r#"(module
  (import "moonkale" "log" (func $log (param i32 i32)))
  (import "moonkale" "call" (func $call (param i32 i32) (result i64)))
  (memory (export "memory") 2)
  (global $bump (mut i32) (i32.const 4096))
  (data (i32.const 0) "{escaped}")
  (data (i32.const 2048) "{extra}")
  (func (export "alloc") (param $n i32) (result i32) (local $p i32)
    global.get $bump local.set $p
    global.get $bump local.get $n i32.add global.set $bump
    local.get $p)
  (func (export "manifest") (result i64) {manifest})
  (func (export "run") (param i32 i32) (result i64) {run_body}))"#
    );
    let path = dir.join(format!("{name}.wasm"));
    std::fs::write(&path, wat::parse_str(&wat).unwrap()).unwrap();
    path
}

fn manifest(id: &str, permissions: &[&str]) -> String {
    serde_json::json!({
        "abi": 1, "id": id, "name": id, "description": "",
        "permissions": permissions,
        "commands": [{ "id": "t.go", "title": "go" }]
    })
    .to_string()
}

#[test]
fn an_endless_manifest_is_stopped() {
    let dir = tempfile::tempdir().unwrap();
    let path = module(
        dir.path(),
        "loop",
        "{}",
        Some("(loop $l br $l) (i64.const 0)"),
        "",
        "(i64.const 0)",
    );
    let mut rt = Runtime::new().unwrap();
    let started = std::time::Instant::now();
    let err = rt.load(&path).err().expect("refused");
    assert!(
        started.elapsed().as_secs() < 30,
        "took {:?}",
        started.elapsed()
    );
    assert!(err.contains("fuel"), "{err}");
}

#[test]
fn an_endless_command_is_stopped() {
    let dir = tempfile::tempdir().unwrap();
    let json = manifest("org.example.loop", &[]);
    let path = module(
        dir.path(),
        "run-loop",
        &json,
        None,
        "",
        "(loop $l br $l) (i64.const 0)",
    );
    let mut rt = Runtime::new().unwrap();
    rt.load(&path).unwrap();
    let tokio = tokio::runtime::Runtime::new().unwrap();
    let err = rt
        .run(
            "org.example.loop",
            "t.go",
            serde_json::Value::Null,
            vec![],
            Arc::new(NoSources),
            tokio.handle().clone(),
        )
        .unwrap_err();
    assert!(err.contains("fuel"), "{err}");
}

#[test]
fn built_in_ids_are_reserved() {
    let dir = tempfile::tempdir().unwrap();
    let json = manifest("dev.moonkale.editor-flow", &[]);
    let path = module(dir.path(), "squat", &json, None, "", "(i64.const 0)");
    let err = Runtime::new().unwrap().load(&path).err().expect("refused");
    assert!(err.contains("reserved"), "{err}");
}

/// `run` asks the host to read 2 GiB at offset 0 and returns the host's reply.
const HUGE_CALL: &str = "(call $call (i32.const 0) (i32.const 0x7fffffff))";

#[test]
fn host_calls_are_bounded_by_the_module_memory() {
    let dir = tempfile::tempdir().unwrap();
    let json = manifest("org.example.huge", &["read-sources"]);
    let path = module(dir.path(), "huge", &json, None, "", HUGE_CALL);
    let mut rt = Runtime::new().unwrap();
    rt.load(&path).unwrap();
    let tokio = tokio::runtime::Runtime::new().unwrap();
    let err = rt
        .run(
            "org.example.huge",
            "t.go",
            serde_json::Value::Null,
            vec!["read-sources".into()],
            Arc::new(NoSources),
            tokio.handle().clone(),
        )
        .unwrap_err();
    assert!(err.contains("out of bounds"), "{err}");
}

/// The module returns the reply of a `list_sources` host call; `granted`
/// holds a permission the module never declared.
#[test]
fn grants_never_exceed_the_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let json = manifest("org.example.modest", &[]);
    let call = r#"{"op":"list_sources"}"#;
    let run = format!("(call $call (i32.const 2048) (i32.const {}))", call.len());
    let path = module(dir.path(), "modest", &json, None, call, &run);
    let mut rt = Runtime::new().unwrap();
    rt.load(&path).unwrap();
    let tokio = tokio::runtime::Runtime::new().unwrap();
    let err = rt
        .run(
            "org.example.modest",
            "t.go",
            serde_json::Value::Null,
            vec!["read-sources".into()],
            Arc::new(NoSources),
            tokio.handle().clone(),
        )
        .unwrap_err();
    assert!(err.contains("not granted"), "{err}");
}
