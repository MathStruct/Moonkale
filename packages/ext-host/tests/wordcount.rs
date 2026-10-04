//! Runs the example extension (built for wasm32 by `build.rs`-less means:
//! the test builds it with cargo on demand) against a temp folder.
#![cfg(feature = "wasmtime")]

use moonkale_core::{Query, Source, SourceDescriptor, SourceId};
use moonkale_ext_host::{Host, Runtime};
use std::path::PathBuf;
use std::sync::Arc;

struct Reg(Vec<Arc<dyn Source>>);
impl Host for Reg {
    fn list_sources(&self) -> Vec<SourceDescriptor> {
        self.0.iter().map(|s| s.descriptor()).collect()
    }
    fn source(&self, id: &SourceId) -> Option<Arc<dyn Source>> {
        self.0.iter().find(|s| &s.id() == id).cloned()
    }
}

fn wasm_path() -> Option<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    // Where the build below puts it: CARGO_TARGET_DIR when set (relative to
    // the workspace root), else <root>/target.
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(|t| root.join(t))
        .unwrap_or_else(|| root.join("target"));
    let p = target.join("wasm32-unknown-unknown/release/moonkale_ext_wordcount.wasm");
    if !p.exists() {
        let ok = std::process::Command::new("cargo")
            .args([
                "build",
                "-p",
                "moonkale-ext-wordcount",
                "--target",
                "wasm32-unknown-unknown",
                "--release",
            ])
            .current_dir(&root)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            eprintln!("wasm32 target unavailable; skipping");
            return None;
        }
    }
    Some(p)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn example_extension_runs_and_permissions_are_enforced() {
    let Some(wasm) = wasm_path() else { return };
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.md"), "the cat and the dog\nthe end\n").unwrap();
    let folder: Arc<dyn Source> =
        Arc::new(moonkale_project_fs::FolderSource::open(dir.path()).unwrap());
    let node = folder
        .query(Query::Children(folder.descriptor().root))
        .await
        .unwrap()
        .nodes[0]
        .id;
    let host: Arc<dyn Host> = Arc::new(Reg(vec![folder.clone()]));

    let mut rt = Runtime::new().unwrap();
    let loaded = rt.load(&wasm).unwrap();
    assert_eq!(loaded.manifest.id, "dev.moonkale.example-wordcount");
    assert_eq!(loaded.manifest.commands.len(), 2);
    assert!(loaded
        .manifest
        .permissions
        .contains(&"read-sources".to_string()));
    let rt = Arc::new(rt);

    let args =
        serde_json::json!({ "source": folder.id().as_str(), "node": node.to_string(), "n": 2 });
    let handle = tokio::runtime::Handle::current();
    // Granted: works.
    let (rt2, host2, args2, h2) = (rt.clone(), host.clone(), args.clone(), handle.clone());
    let out = tokio::task::spawn_blocking(move || {
        rt2.run(
            "dev.moonkale.example-wordcount",
            "wordcount.count",
            args2,
            vec!["read-sources".into()],
            host2,
            h2,
        )
    })
    .await
    .unwrap()
    .unwrap();
    assert_eq!(out, "2 lines, 7 words, 28 characters");
    let (rt2, host2, args2, h2) = (rt.clone(), host.clone(), args.clone(), handle.clone());
    let top = tokio::task::spawn_blocking(move || {
        rt2.run(
            "dev.moonkale.example-wordcount",
            "wordcount.top",
            args2,
            vec!["read-sources".into()],
            host2,
            h2,
        )
    })
    .await
    .unwrap()
    .unwrap();
    assert_eq!(top, "the: 3\nand: 1");
    // Not granted: the host call is refused and the extension reports it.
    let (rt2, host2, args2, h2) = (rt.clone(), host.clone(), args.clone(), handle.clone());
    let err = tokio::task::spawn_blocking(move || {
        rt2.run(
            "dev.moonkale.example-wordcount",
            "wordcount.count",
            args2,
            vec![],
            host2,
            h2,
        )
    })
    .await
    .unwrap()
    .unwrap_err();
    assert!(err.contains("permission read-sources not granted"), "{err}");
    // Unknown command.
    let (rt2, host2, h2) = (rt.clone(), host.clone(), handle.clone());
    let err = tokio::task::spawn_blocking(move || {
        rt2.run(
            "dev.moonkale.example-wordcount",
            "wordcount.nope",
            serde_json::json!({}),
            vec![],
            host2,
            h2,
        )
    })
    .await
    .unwrap()
    .unwrap_err();
    assert!(err.contains("no command"));
}

/// Audit #4: a second file with the same id — a folder's copy of a module
/// the user installed — is refused, and the first keeps the id.
#[test]
fn an_id_belongs_to_the_file_that_loaded_it_first() {
    let Some(wasm) = wasm_path() else { return };
    let dir = tempfile::tempdir().unwrap();
    let squatter = dir.path().join("squatter.wasm");
    std::fs::copy(&wasm, &squatter).unwrap();
    let mut rt = Runtime::new().unwrap();
    rt.load(&wasm).unwrap();
    let err = rt
        .load(&squatter)
        .err()
        .expect("the second file is refused");
    assert!(err.contains("is taken by"), "{err}");
    assert_eq!(rt.extensions.len(), 1);
    assert_eq!(rt.extensions[0].path, wasm);
    // The first file reloads.
    rt.load(&wasm).unwrap();
    assert_eq!(rt.extensions.len(), 1);
}
