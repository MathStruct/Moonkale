//! Real rust-analyzer diagnostics and editing-feature acceptance.
//! Skipped when no server is installed.
use futures_util::StreamExt;
use moonkale_lsp::{LspEvent, LspSession};
use moonkale_lsp_local::{discover, StdioTransport};
use std::time::Duration;

#[tokio::test]
async fn diagnostics_and_editing_features() {
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
                        ..
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
            // Exercise the same protocol client as the native panel against a
            // live server, including UTF-16 positions after an emoji and CRLF.
            let text = "fn greet() -> i32 { 7 }\r\nfn main() { let _ = \"😀\"; let value = greet(); let _ = value; }\r\n";
            session.did_change(&uri, 2, text);
            tokio::time::timeout(Duration::from_secs(30), async {
                // didChange is processed in order before subsequent requests.
                let line = text.lines().nth(1).unwrap();
                let call = line[..line.find("greet()").unwrap()].encode_utf16().count() as u32;
                let value = line[..line.rfind("value").unwrap()].encode_utf16().count() as u32;
                let hover = session.hover(&uri, 1, call).await.unwrap().unwrap();
                assert!(hover.contains("greet"), "{hover}");
                let definition = session.definition(&uri, 1, call).await.unwrap().unwrap();
                assert_eq!(definition.uri, uri);
                assert_eq!(definition.line, 0);
                assert_eq!(definition.col, 3);
                let references = session.references(&uri, 1, call).await.unwrap();
                assert!(references.iter().any(|r| r.uri == uri && r.line == 0 && r.col == 3));
                assert!(references.iter().any(|r| r.uri == uri && r.line == 1 && r.col == call));
                let rename = session.rename(&uri, 1, value, "answer").await.unwrap();
                let edits = &rename.changes.iter().find(|(u, _)| u == &uri).unwrap().1;
                assert_eq!(edits.len(), 2);
                assert!(edits.iter().all(|e| e.new_text == "answer" && e.line == 1));
                assert!(edits.iter().any(|e| e.col == value && e.end_col == value + 5));
                let completion = session.completion(&uri, 1, call + 2).await.unwrap();
                assert!(completion.iter().any(|item| item.label.contains("greet")), "{completion:?}");
                let actions = session.code_actions(&uri, 0, 3, 0, 8).await.unwrap();
                let action = actions.iter().find(|action| action.disabled.is_none())
                    .expect("expected an enabled function refactoring");
                let action_edit = match &action.edit {
                    Some(edit) => edit.clone(),
                    None => session.resolve_code_action(action).await.unwrap(),
                };
                assert!(action_edit.changes.iter().any(|(u, edits)| u == &uri && !edits.is_empty()));
                session.did_save(&uri);
                session.did_close(&uri);
            }).await.expect("real-server feature acceptance timed out");
        })
        .await;
}
