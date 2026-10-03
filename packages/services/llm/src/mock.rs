//! A provider for tests and for running without any model: deterministic,
//! offline, and scriptable from the chat itself.
//!
//! Scripted behaviour (`MockProvider::scripted`):
//! - a user message `/tool <name> <json>` → the model "calls" that tool;
//! - after a tool result → "Tool `<name>` returned: <first 400 chars>";
//! - anything else → "mock: <the user's text>".
//!
//! Embeddings are a bag-of-characters hash into 32 dimensions, so equal
//! texts are identical and similar texts are close — enough to test the
//! vector path end to end.

use crate::provider::{BoxFuture, EventStream, Provider};
use crate::types::{Content, Event, Request, Role, StopReason, Usage};
use std::sync::Mutex;

pub struct MockProvider {
    /// Fixed event lists to play back, one per `complete` call (tests).
    canned: Mutex<Vec<Vec<Event>>>,
}

impl MockProvider {
    pub fn scripted() -> Self {
        Self {
            canned: Mutex::new(Vec::new()),
        }
    }

    /// Play back these turns (first call gets `turns[0]`, …), then fall back
    /// to the scripted behaviour.
    pub fn canned(turns: Vec<Vec<Event>>) -> Self {
        Self {
            canned: Mutex::new(turns.into_iter().rev().collect()),
        }
    }

    fn script(request: &Request) -> Vec<Event> {
        let done = |stop| Event::Done {
            stop,
            usage: Usage {
                input_tokens: 1,
                output_tokens: 1,
            },
        };
        let Some(last) = request.messages.last() else {
            return vec![
                Event::TextDelta {
                    text: "mock: (empty)".into(),
                },
                done(StopReason::EndTurn),
            ];
        };
        if last.role == Role::User {
            if let Some(Content::ToolResult { output, .. }) = last
                .content
                .iter()
                .find(|c| matches!(c, Content::ToolResult { .. }))
            {
                let name = request
                    .messages
                    .iter()
                    .rev()
                    .find_map(|m| {
                        m.content.iter().find_map(|c| match c {
                            Content::ToolUse { name, .. } => Some(name.clone()),
                            _ => None,
                        })
                    })
                    .unwrap_or_default();
                let head: String = output.chars().take(1500).collect();
                return vec![
                    Event::TextDelta {
                        text: format!("Tool `{name}` returned: {head}"),
                    },
                    done(StopReason::EndTurn),
                ];
            }
            let text = last.text();
            if let Some(rest) = text.trim().strip_prefix("/tool ") {
                let (name, json) = rest.split_once(' ').unwrap_or((rest, "{}"));
                let input = serde_json::from_str(json.trim()).unwrap_or(serde_json::json!({}));
                return vec![
                    Event::TextDelta {
                        text: format!("Calling {name}… "),
                    },
                    Event::ToolUse {
                        id: format!("mock-{}", request.messages.len()),
                        name: name.to_string(),
                        input,
                    },
                    done(StopReason::ToolUse),
                ];
            }
            return vec![
                Event::TextDelta {
                    text: format!("mock: {text}"),
                },
                done(StopReason::EndTurn),
            ];
        }
        vec![done(StopReason::EndTurn)]
    }
}

impl Provider for MockProvider {
    fn name(&self) -> String {
        "mock".into()
    }
    fn model(&self) -> String {
        "mock".into()
    }
    fn complete(&self, request: Request) -> EventStream {
        let (tx, rx) = futures_channel::mpsc::unbounded();
        let events = self
            .canned
            .lock()
            .unwrap()
            .pop()
            .unwrap_or_else(|| Self::script(&request));
        for e in events {
            let _ = tx.unbounded_send(e);
        }
        rx
    }
    fn embed(&self, texts: Vec<String>) -> BoxFuture<Result<Vec<Vec<f32>>, String>> {
        Box::pin(async move { Ok(texts.iter().map(|t| mock_embedding(t)).collect()) })
    }
    fn supports_embed(&self) -> bool {
        true
    }
}

/// Bag-of-words hashed into 32 dimensions, L2-normalised.
pub fn mock_embedding(text: &str) -> Vec<f32> {
    let mut v = vec![0f32; 32];
    for word in text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
    {
        let h = word
            .to_lowercase()
            .bytes()
            .fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619));
        v[(h % 32) as usize] += 1.0;
    }
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-6);
    v.iter_mut().for_each(|x| *x /= norm);
    v
}
