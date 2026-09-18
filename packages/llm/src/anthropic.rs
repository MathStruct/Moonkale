//! Anthropic Messages API (`POST /v1/messages`, `stream: true`).
//! Our message model maps 1:1 (content blocks, tool_use / tool_result).

use crate::provider::{BoxFuture, EventStream, Provider};
use crate::sse::SseParser;
use crate::types::{Content, Event, Message, Request, Role, StopReason, Usage};
use futures_util::StreamExt;
use serde_json::{json, Value};

pub struct Anthropic {
    api_key: String,
    model: String,
    base_url: String,
    client: reqwest::Client,
}

impl Anthropic {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            api_key,
            model,
            base_url: std::env::var("ANTHROPIC_BASE_URL")
                .unwrap_or_else(|_| "https://api.anthropic.com".into()),
            client: reqwest::Client::new(),
        }
    }

    pub fn body(&self, req: &Request) -> Value {
        let messages: Vec<Value> = req.messages.iter().map(message_json).collect();
        let mut body = json!({
            "model": req.model.clone().unwrap_or_else(|| self.model.clone()),
            "max_tokens": req.max_tokens.max(1),
            "messages": messages,
            "stream": true,
        });
        if !req.system.is_empty() {
            body["system"] = json!(req.system);
        }
        if !req.tools.is_empty() {
            body["tools"] = json!(req
                .tools
                .iter()
                .map(|t| json!({ "name": t.name, "description": t.description, "input_schema": t.input_schema }))
                .collect::<Vec<_>>());
        }
        body
    }
}

fn message_json(m: &Message) -> Value {
    let role = match m.role {
        Role::User => "user",
        Role::Assistant => "assistant",
    };
    let content: Vec<Value> = m
        .content
        .iter()
        .map(|c| match c {
            Content::Text { text } => json!({ "type": "text", "text": text }),
            Content::ToolUse { id, name, input } => {
                json!({ "type": "tool_use", "id": id, "name": name, "input": input })
            }
            Content::ToolResult {
                id,
                output,
                is_error,
            } => json!({ "type": "tool_result", "tool_use_id": id, "content": output, "is_error": is_error }),
        })
        .collect();
    json!({ "role": role, "content": content })
}

/// Translates the SSE event stream into our events. Tool input JSON arrives
/// as deltas; it is assembled and emitted at `content_block_stop`.
#[derive(Default)]
pub struct Translator {
    block: Option<(String, String, String)>, // (id, name, partial json)
    stop: StopReason,
    usage: Usage,
    done: bool,
}

impl Translator {
    pub fn on_event(&mut self, event: Option<&str>, data: &str) -> Vec<Event> {
        let Ok(v) = serde_json::from_str::<Value>(data) else {
            return Vec::new();
        };
        let ty = event
            .map(str::to_string)
            .or_else(|| v["type"].as_str().map(str::to_string))
            .unwrap_or_default();
        match ty.as_str() {
            "message_start" => {
                if let Some(u) = v["message"]["usage"]["input_tokens"].as_u64() {
                    self.usage.input_tokens = u as u32;
                }
                Vec::new()
            }
            "content_block_start" => {
                let b = &v["content_block"];
                if b["type"] == "tool_use" {
                    self.block = Some((
                        b["id"].as_str().unwrap_or_default().to_string(),
                        b["name"].as_str().unwrap_or_default().to_string(),
                        String::new(),
                    ));
                }
                Vec::new()
            }
            "content_block_delta" => {
                let d = &v["delta"];
                if d["type"] == "text_delta" {
                    return vec![Event::TextDelta {
                        text: d["text"].as_str().unwrap_or_default().to_string(),
                    }];
                }
                if d["type"] == "input_json_delta" {
                    if let Some(b) = &mut self.block {
                        b.2.push_str(d["partial_json"].as_str().unwrap_or_default());
                    }
                }
                Vec::new()
            }
            "content_block_stop" => match self.block.take() {
                Some((id, name, raw)) => {
                    let input = if raw.trim().is_empty() {
                        json!({})
                    } else {
                        serde_json::from_str(&raw).unwrap_or(json!({}))
                    };
                    vec![Event::ToolUse { id, name, input }]
                }
                None => Vec::new(),
            },
            "message_delta" => {
                self.stop = match v["delta"]["stop_reason"].as_str() {
                    Some("end_turn") | Some("stop_sequence") => StopReason::EndTurn,
                    Some("tool_use") => StopReason::ToolUse,
                    Some("max_tokens") => StopReason::MaxTokens,
                    _ => StopReason::Other,
                };
                if let Some(u) = v["usage"]["output_tokens"].as_u64() {
                    self.usage.output_tokens = u as u32;
                }
                Vec::new()
            }
            "message_stop" => {
                self.done = true;
                vec![Event::Done {
                    stop: self.stop,
                    usage: self.usage,
                }]
            }
            "error" => vec![Event::Error {
                message: v["error"]["message"]
                    .as_str()
                    .unwrap_or("provider error")
                    .to_string(),
            }],
            _ => Vec::new(),
        }
    }

    pub fn is_done(&self) -> bool {
        self.done
    }
}

impl Provider for Anthropic {
    fn name(&self) -> String {
        "anthropic".into()
    }
    fn model(&self) -> String {
        self.model.clone()
    }
    fn complete(&self, request: Request) -> EventStream {
        let (tx, rx) = futures_channel::mpsc::unbounded();
        let body = self.body(&request);
        let req = self
            .client
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body);
        tokio::spawn(async move {
            let resp = match req.send().await {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx.unbounded_send(Event::Error {
                        message: e.to_string(),
                    });
                    return;
                }
            };
            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                let _ = tx.unbounded_send(Event::Error {
                    message: format!("{status}: {}", text.chars().take(500).collect::<String>()),
                });
                return;
            }
            let mut parser = SseParser::new();
            let mut tr = Translator::default();
            let mut stream = resp.bytes_stream();
            while let Some(chunk) = stream.next().await {
                let chunk = match chunk {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx.unbounded_send(Event::Error {
                            message: e.to_string(),
                        });
                        return;
                    }
                };
                for ev in parser.feed(&String::from_utf8_lossy(&chunk)) {
                    for out in tr.on_event(ev.event.as_deref(), &ev.data) {
                        let _ = tx.unbounded_send(out);
                    }
                }
            }
            if !tr.is_done() {
                let _ = tx.unbounded_send(Event::Done {
                    stop: StopReason::Other,
                    usage: Usage::default(),
                });
            }
        });
        rx
    }
    fn embed(&self, _texts: Vec<String>) -> BoxFuture<Result<Vec<Vec<f32>>, String>> {
        Box::pin(async {
            Err("Anthropic has no embeddings API; set OPENAI_* or OLLAMA_HOST for MOONKALE_EMBED_MODEL".into())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_a_tool_use_stream() {
        let recorded = [
            (
                "message_start",
                r#"{"type":"message_start","message":{"usage":{"input_tokens":25}}}"#,
            ),
            (
                "content_block_start",
                r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
            ),
            (
                "content_block_delta",
                r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Let me look. "}}"#,
            ),
            (
                "content_block_stop",
                r#"{"type":"content_block_stop","index":0}"#,
            ),
            (
                "content_block_start",
                r#"{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"toolu_1","name":"index.search","input":{}}}"#,
            ),
            (
                "content_block_delta",
                r#"{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"query\": \"hel"}}"#,
            ),
            (
                "content_block_delta",
                r#"{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"lo\"}"}}"#,
            ),
            (
                "content_block_stop",
                r#"{"type":"content_block_stop","index":1}"#,
            ),
            (
                "message_delta",
                r#"{"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":17}}"#,
            ),
            ("message_stop", r#"{"type":"message_stop"}"#),
        ];
        let mut tr = Translator::default();
        let out: Vec<Event> = recorded
            .iter()
            .flat_map(|(e, d)| tr.on_event(Some(e), d))
            .collect();
        assert_eq!(
            out,
            vec![
                Event::TextDelta {
                    text: "Let me look. ".into()
                },
                Event::ToolUse {
                    id: "toolu_1".into(),
                    name: "index.search".into(),
                    input: json!({"query": "hello"})
                },
                Event::Done {
                    stop: StopReason::ToolUse,
                    usage: Usage {
                        input_tokens: 25,
                        output_tokens: 17
                    }
                },
            ]
        );
    }

    #[test]
    fn request_body_shape() {
        let a = Anthropic::new("k".into(), "claude-x".into());
        let req = Request {
            system: "sys".into(),
            messages: vec![Message::user("hi")],
            max_tokens: 100,
            ..Default::default()
        };
        let b = a.body(&req);
        assert_eq!(b["model"], "claude-x");
        assert_eq!(b["system"], "sys");
        assert_eq!(b["messages"][0]["content"][0]["text"], "hi");
        assert!(b.get("tools").is_none());
    }
}
