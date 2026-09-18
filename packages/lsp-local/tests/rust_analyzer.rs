//! Talks to a real rust-analyzer. Skipped when none is installed.
use futures_util::StreamExt;
use moonkale_lsp::{LspEvent, LspSession};
use moonkale_lsp_local::{discover, StdioTransport};
use std::time::Duration;

#[tokio::test]
async fn diagnostics_for_a_broken_crate() {
    let Some(spec) = discover::find("rust") else {
        eprintln!("rust-analyzer not installed; skipping");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"t\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::create_dir(dir.path().join("src")).unwrap();
    let main = dir.path().join("src/main.rs");
    std::fs::write(&main, "fn main() { let x: i32 = \"str\"; }\n").unwrap();
    let root = dir.path().to_string_lossy().into_owned();
    let args: Vec<&str> = spec.args.iter().map(String::as_str).collect();
    let transport = StdioTransport::spawn(&spec.program, &args, &root).unwrap();
    let (session, mut events) = LspSession::new(Box::new(transport));
    let local = tokio::task::LocalSet::new();
    local.spawn_local(session.clone().pump());
    local
        .run_until(async move {
            let server = tokio::time::timeout(Duration::from_secs(30), session.initialize(&root))
                .await
                .expect("initialize timed out")
                .unwrap();
            assert!(server.to_lowercase().contains("rust"), "{server}");
            let uri = format!("file://{}", main.display());
            session.did_open(&uri, "rust", 1, &std::fs::read_to_string(&main).unwrap());
            let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
            loop {
                let ev = tokio::time::timeout_at(deadline, events.next())
                    .await
                    .expect("no diagnostics within 120s");
                match ev {
                    Some(LspEvent::Diagnostics {
                        uri: u,
                        diagnostics,
                    }) if u == uri && !diagnostics.is_empty() => {
                        assert!(
                            diagnostics.iter().any(|d| d.severity == "error"),
                            "{diagnostics:?}"
                        );
                        break;
                    }
                    Some(LspEvent::Closed) | None => panic!("server closed"),
                    _ => {}
                }
            }
        })
        .await;
}
