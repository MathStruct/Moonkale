//! OpenAI-compatible chat completions (`POST {base}/chat/completions`,
//! `stream: true`) with tool calling, plus `POST {base}/embeddings`. Also
//! used for **Ollama** (`{host}/v1`), whose embeddings go through its
//! native `POST {host}/api/embed`.

use crate::provider::{BoxFuture, EventStream, Provider};
use crate::sse::SseParser;
use crate::types::{Content, Event, Message, Request, Role, StopReason, Usage};
use futures_util::StreamExt;
use serde_json::{json, Value};

pub struct OpenAi {
    base_url: String,
    api_key: Option<String>,
    model: String,
    embed_model: Option<String>,
    /// Ollama host for native embeddings; `None` = OpenAI embeddings endpoint.
    ollama_host: Option<String>,
    client: reqwest::Client,
}

impl OpenAi {
    pub fn new(
        base_url: String,
        api_key: Option<String>,
        model: String,
        embed_model: Option<String>,
    ) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            model,
            embed_model,
            ollama_host: None,
            client: reqwest::Client::new(),
        }
    }

    pub fn ollama(host: String, model: String, embed_model: Option<String>) -> Self {
        let host = host.trim_end_matches('/').to_string();
        let mut s = Self::new(format!("{host}/v1"), None, model, embed_model);
        s.ollama_host = Some(host);
        s
    }

    pub fn body(&self, req: &Request) -> Value {
        let mut messages = Vec::new();
        if !req.system.is_empty() {
            messages.push(json!({ "role": "system", "content": req.system }));
        }
        for m in &req.messages {
            messages.extend(message_json(m));
        }
        let mut body = json!({
            "model": req.model.clone().unwrap_or_else(|| self.model.clone()),
            "messages": messages,
            "stream": true,
            "max_tokens": req.max_tokens.max(1),
        });
        if !req.tools.is_empty() {
            body["tools"] = json!(req
                .tools
                .iter()
                .map(|t| json!({ "type": "function", "function": { "name": t.name, "description": t.description, "parameters": t.input_schema } }))
                .collect::<Vec<_>>());
        }
        body
    }
}

/// One of our messages may become several OpenAI messages (tool results
/// are separate `role: tool` messages).
fn message_json(m: &Message) -> Vec<Value> {
    match m.role {
        Role::Assistant => {
            let text = m.text();
            let calls: Vec<Value> = m
                .content
                .iter()
                .filter_map(|c| match c {
                    Content::ToolUse { id, name, input } => Some(json!({
                        "id": id, "type": "function",
                        "function": { "name": name, "arguments": input.to_string() }
                    })),
                    _ => None,
                })
                .collect();
            let mut v = json!({ "role": "assistant", "content": text });
            if !calls.is_empty() {
                v["tool_calls"] = json!(calls);
            }
            vec![v]
        }
        Role::User => {
            let mut out = Vec::new();
            let mut text = String::new();
            for c in &m.content {
                match c {
                    Content::Text { text: t } => text.push_str(t),
                    Content::ToolResult { id, output, .. } => {
                        out.push(json!({ "role": "tool", "tool_call_id": id, "content": output }))
                    }
                    Content::ToolUse { .. } => {}
                }
            }
            if !text.is_empty() || out.is_empty() {
                out.push(json!({ "role": "user", "content": text }));
            }
            out
        }
    }
}

/// Assembles streamed `tool_calls` deltas (by index) and emits them whole
/// when the stream finishes.
#[derive(Default)]
pub struct Translator {
    calls: Vec<(String, String, String)>, // (id, name, arguments)
    stop: Option<StopReason>,
    usage: Usage,
    done: bool,
}

impl Translator {
    pub fn on_data(&mut self, data: &str) -> Vec<Event> {
        if data.trim() == "[DONE]" {
            return self.finish();
        }
        let Ok(v) = serde_json::from_str::<Value>(data) else {
            return Vec::new();
        };
        if let Some(err) = v.get("error") {
            return vec![Event::Error {
                message: err["message"]
                    .as_str()
                    .unwrap_or("provider error")
                    .to_string(),
            }];
        }
        if let Some(u) = v.get("usage").filter(|u| !u.is_null()) {
            self.usage.input_tokens = u["prompt_tokens"].as_u64().unwrap_or(0) as u32;
            self.usage.output_tokens = u["completion_tokens"].as_u64().unwrap_or(0) as u32;
        }
        let mut out = Vec::new();
        for choice in v["choices"].as_array().into_iter().flatten() {
            let delta = &choice["delta"];
            if let Some(t) = delta["content"].as_str().filter(|t| !t.is_empty()) {
                out.push(Event::TextDelta {
                    text: t.to_string(),
                });
            }
            for tc in delta["tool_calls"].as_array().into_iter().flatten() {
                let idx = tc["index"].as_u64().unwrap_or(0) as usize;
                while self.calls.len() <= idx {
                    self.calls.push(Default::default());
                }
                let slot = &mut self.calls[idx];
                if let Some(id) = tc["id"].as_str() {
                    slot.0 = id.to_string();
                }
                if let Some(n) = tc["function"]["name"].as_str() {
                    slot.1.push_str(n);
                }
                if let Some(a) = tc["function"]["arguments"].as_str() {
                    slot.2.push_str(a);
                }
            }
            match choice["finish_reason"].as_str() {
                Some("stop") => self.stop = Some(StopReason::EndTurn),
                Some("tool_calls") | Some("function_call") => self.stop = Some(StopReason::ToolUse),
                Some("length") => self.stop = Some(StopReason::MaxTokens),
                _ => {}
            }
        }
        out
    }

    fn finish(&mut self) -> Vec<Event> {
        if self.done {
            return Vec::new();
        }
        self.done = true;
        let mut out = Vec::new();
        for (i, (id, name, args)) in self.calls.drain(..).enumerate() {
            if name.is_empty() {
                continue;
            }
            let input = if args.trim().is_empty() {
                json!({})
            } else {
                serde_json::from_str(&args).unwrap_or(json!({}))
            };
            out.push(Event::ToolUse {
                id: if id.is_empty() {
                    format!("call_{i}")
                } else {
                    id
                },
                name,
                input,
            });
        }
        let stop = self.stop.unwrap_or(if out.is_empty() {
            StopReason::EndTurn
        } else {
            StopReason::ToolUse
        });
        out.push(Event::Done {
            stop,
            usage: self.usage,
        });
        out
    }

    pub fn is_done(&self) -> bool {
        self.done
    }
}

impl Provider for OpenAi {
    fn name(&self) -> String {
        if self.ollama_host.is_some() {
            "ollama".into()
        } else {
            "openai".into()
        }
    }
    fn model(&self) -> String {
        self.model.clone()
    }
    fn complete(&self, request: Request) -> EventStream {
        let (tx, rx) = futures_channel::mpsc::unbounded();
        let body = self.body(&request);
        let mut req = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("content-type", "application/json");
        if let Some(k) = &self.api_key {
            req = req.bearer_auth(k);
        }
        let req = req.json(&body);
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
                    for out in tr.on_data(&ev.data) {
                        let _ = tx.unbounded_send(out);
                    }
                }
            }
            if let Some(ev) = parser.finish() {
                for out in tr.on_data(&ev.data) {
                    let _ = tx.unbounded_send(out);
                }
            }
            if !tr.is_done() {
                for out in tr.finish() {
                    let _ = tx.unbounded_send(out);
                }
            }
        });
        rx
    }
    fn embed(&self, texts: Vec<String>) -> BoxFuture<Result<Vec<Vec<f32>>, String>> {
        let Some(model) = self.embed_model.clone() else {
            return Box::pin(async { Err("no MOONKALE_EMBED_MODEL configured".into()) });
        };
        let client = self.client.clone();
        let (url, body, key) = match &self.ollama_host {
            Some(host) => (
                format!("{host}/api/embed"),
                json!({ "model": model, "input": texts }),
                None,
            ),
            None => (
                format!("{}/embeddings", self.base_url),
                json!({ "model": model, "input": texts }),
                self.api_key.clone(),
            ),
        };
        let native = self.ollama_host.is_some();
        Box::pin(async move {
            let mut req = client.post(url).json(&body);
            if let Some(k) = key {
                req = req.bearer_auth(k);
            }
            let resp = req.send().await.map_err(|e| e.to_string())?;
            if !resp.status().is_success() {
                return Err(format!(
                    "{}: {}",
                    resp.status(),
                    resp.text()
                        .await
                        .unwrap_or_default()
                        .chars()
                        .take(300)
                        .collect::<String>()
                ));
            }
            let v: Value = resp.json().await.map_err(|e| e.to_string())?;
            let rows = if native {
                v["embeddings"].as_array().cloned().unwrap_or_default()
            } else {
                v["data"]
                    .as_array()
                    .map(|d| d.iter().map(|e| e["embedding"].clone()).collect())
                    .unwrap_or_default()
            };
            Ok(rows
                .iter()
                .map(|r| {
                    r.as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|x| x.as_f64().map(|f| f as f32))
                                .collect()
                        })
                        .unwrap_or_default()
                })
                .collect())
        })
    }
    fn supports_embed(&self) -> bool {
        self.embed_model.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_streamed_tool_calls() {
        let chunks = [
            r#"{"choices":[{"delta":{"role":"assistant","content":""},"finish_reason":null}]}"#,
            r#"{"choices":[{"delta":{"content":"Sure. "},"finish_reason":null}]}"#,
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_9","type":"function","function":{"name":"index.search","arguments":""}}]},"finish_reason":null}]}"#,
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"query\":"}}]},"finish_reason":null}]}"#,
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"hi\"}"}}]},"finish_reason":null}]}"#,
            r#"{"choices":[{"delta":{},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":10,"completion_tokens":5}}"#,
            "[DONE]",
        ];
        let mut tr = Translator::default();
        let out: Vec<Event> = chunks.iter().flat_map(|c| tr.on_data(c)).collect();
        assert_eq!(
            out,
            vec![
                Event::TextDelta {
                    text: "Sure. ".into()
                },
                Event::ToolUse {
                    id: "call_9".into(),
                    name: "index.search".into(),
                    input: json!({"query": "hi"})
                },
                Event::Done {
                    stop: StopReason::ToolUse,
                    usage: Usage {
                        input_tokens: 10,
                        output_tokens: 5
                    }
                },
            ]
        );
    }

    #[test]
    fn tool_results_become_tool_messages() {
        let o = OpenAi::new("http://x/v1".into(), None, "m".into(), None);
        let req = Request {
            messages: vec![
                Message::user("q"),
                Message::assistant(vec![Content::ToolUse {
                    id: "c1".into(),
                    name: "t".into(),
                    input: json!({"a":1}),
                }]),
                Message::tool_results(vec![Content::ToolResult {
                    id: "c1".into(),
                    output: "out".into(),
                    is_error: false,
                }]),
            ],
            ..Default::default()
        };
        let b = o.body(&req);
        let roles: Vec<&str> = b["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["role"].as_str().unwrap())
            .collect();
        assert_eq!(roles, vec!["user", "assistant", "tool"]);
        assert_eq!(
            b["messages"][1]["tool_calls"][0]["function"]["arguments"],
            "{\"a\":1}"
        );
        assert_eq!(b["messages"][2]["tool_call_id"], "c1");
    }
}
