//! The `claude-code` provider against the mock CLI (`tests/mock-claude.sh`):
//! spawn, stream-json → events, cwd, permission mode, `--resume` on the
//! second turn, an error result.
#![cfg(feature = "claude-code")]

use futures_util::StreamExt;
use moonkale_llm::{Content, Event, Message, Provider, Request, StopReason};

fn mock() -> String {
    concat!(env!("CARGO_MANIFEST_DIR"), "/tests/mock-claude.sh").to_string()
}

async fn collect(mut s: moonkale_llm::provider::EventStream) -> (String, Option<Event>) {
    let mut text = String::new();
    let mut last = None;
    while let Some(e) = s.next().await {
        match e {
            Event::TextDelta { text: t } => text.push_str(&t),
            other => last = Some(other),
        }
    }
    (text, last)
}

#[tokio::test]
async fn two_turns_resume_and_show_tool_lines() {
    let p = moonkale_llm::claude_code::ClaudeCode::new(
        mock(),
        String::new(),
        "plan".into(),
        String::new(),
    );
    let dir = std::env::temp_dir().join("moonkale-cc-test");
    std::fs::create_dir_all(&dir).unwrap();
    let cwd = Some(dir.to_string_lossy().into_owned());
    let (text, last) = collect(p.complete(Request {
        cwd: cwd.clone(),
        messages: vec![Message::user("hello")],
        ..Default::default()
    }))
    .await;
    assert!(text.contains("▸ Read README.md"), "{text}");
    assert!(
        text.contains("mock claude: hello [cwd=moonkale-cc-test, mode=plan, resumed=no]"),
        "{text}"
    );
    assert!(
        matches!(last, Some(Event::Done { stop: StopReason::EndTurn, usage }) if usage.output_tokens == 7)
    );
    let (text, _) = collect(p.complete(Request {
        cwd,
        messages: vec![
            Message::user("hello"),
            Message::assistant(vec![Content::Text { text: "hi".into() }]),
            Message::user("again"),
        ],
        ..Default::default()
    }))
    .await;
    assert!(
        text.contains("mock claude: again [cwd=moonkale-cc-test, mode=plan, resumed=yes]"),
        "{text}"
    );
}

#[tokio::test]
async fn an_error_result_is_an_error_event() {
    let p = moonkale_llm::claude_code::ClaudeCode::new(
        mock(),
        String::new(),
        String::new(),
        String::new(),
    );
    let (_, last) = collect(p.complete(Request {
        messages: vec![Message::user("please MOCK_FAIL")],
        ..Default::default()
    }))
    .await;
    assert!(matches!(last, Some(Event::Error { message }) if message == "mock failure"));
}

#[tokio::test]
async fn a_missing_binary_is_a_readable_error() {
    let p = moonkale_llm::claude_code::ClaudeCode::new(
        "/nonexistent/claude".into(),
        String::new(),
        String::new(),
        String::new(),
    );
    let (_, last) = collect(p.complete(Request {
        messages: vec![Message::user("x")],
        ..Default::default()
    }))
    .await;
    assert!(matches!(last, Some(Event::Error { message }) if message.contains("claude login")));
}
