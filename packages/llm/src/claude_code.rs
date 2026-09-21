//! Claude Code as a provider (Milestone 12, [[Claude Code Extension]]
//! Level 3): every turn spawns the `claude` CLI headless in the folder —
//!
//! `claude -p <prompt> --output-format stream-json --verbose
//!    --permission-mode <mode> [--allowedTools a,b] [--model m]
//!    --session-id <new> | --resume <id>`
//!
//! — and maps its JSON lines onto [`Event`]s. The CLI authenticates with
//! the subscription login in `~/.claude/` (never `--bare`, which would turn
//! that off); Moonkale sees no credential. Claude Code runs its **own**
//! tools inside `cwd`; this provider never emits `ToolUse`, so the agent
//! loop is a single round, and the CLI's tool activity shows in the
//! transcript as `▸ Read src/main.rs` lines.
//!
//! Continuity: `(session id, user turns seen)` per folder. One more user
//! message than last time → `--resume`; anything else (a cleared chat, a
//! different folder) → a fresh session whose first prompt carries the
//! earlier turns as context.
//!
//! Shapes below are what `claude` 2.1.278 emits (probed 2026-09-21):
//! `system/init` (session_id, model), `rate_limit_event`, `assistant`
//! (message.content: text | tool_use), `user` (tool_result), `result`
//! (`subtype`, `is_error`, `result`, `usage`). Unknown kinds are logged
//! and skipped.

use crate::provider::{BoxFuture, EventStream, Provider};
use crate::types::{Content, Event, Message, Request, Role, StopReason, Usage};
use serde_json::Value;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Mutex;
use std::time::Duration;

/// A turn may take this long before the CLI is killed.
const TURN_TIMEOUT: Duration = Duration::from_secs(600);

pub struct ClaudeCode {
    path: String,
    model: String,
    permission_mode: String,
    allowed_tools: String,
    /// cwd → (session id, user turns the session has seen).
    sessions: Mutex<HashMap<String, (String, usize)>>,
}

impl ClaudeCode {
    pub fn new(
        path: String,
        model: String,
        permission_mode: String,
        allowed_tools: String,
    ) -> Self {
        Self {
            path: if path.trim().is_empty() {
                std::env::var("MOONKALE_CLAUDE_BIN").unwrap_or_else(|_| "claude".into())
            } else {
                path
            },
            model,
            permission_mode: if permission_mode.trim().is_empty() {
                "plan".into()
            } else {
                permission_mode
            },
            allowed_tools,
            sessions: Mutex::new(HashMap::new()),
        }
    }

    /// The prompt for this turn and whether to resume: the last user text,
    /// or — for a session that does not continue the previous one — the
    /// whole conversation rendered as context plus the last text.
    fn plan_turn(&self, cwd: &str, messages: &[Message]) -> (String, String, bool) {
        let users: Vec<&Message> = messages
            .iter()
            .filter(|m| {
                m.role == Role::User && m.content.iter().any(|c| matches!(c, Content::Text { .. }))
            })
            .collect();
        let turns = users.len();
        let last = users.last().map(|m| m.text()).unwrap_or_default();
        let mut map = self.sessions.lock().unwrap();
        if let Some((id, seen)) = map.get(cwd).cloned() {
            if turns == seen + 1 {
                map.insert(cwd.to_string(), (id.clone(), turns));
                return (id, last, true);
            }
        }
        let id = uuid::Uuid::new_v4().to_string();
        map.insert(cwd.to_string(), (id.clone(), turns));
        let prompt = if turns > 1 {
            let mut ctx = String::from("Earlier in this conversation:\n\n");
            for m in messages
                .iter()
                .filter(|m| m.content.iter().any(|c| matches!(c, Content::Text { .. })))
            {
                let who = match m.role {
                    Role::User => "User",
                    Role::Assistant => "Assistant",
                };
                let t = m.text();
                if t.trim().is_empty() || std::ptr::eq(m, *users.last().unwrap()) {
                    continue;
                }
                ctx.push_str(&format!("{who}: {t}\n\n"));
            }
            format!("{ctx}Now the user says:\n\n{last}")
        } else {
            last
        };
        (id, prompt, false)
    }
}

/// One line of `▸ Tool target` for a tool_use block.
pub fn tool_line(name: &str, input: &Value) -> String {
    let target = [
        "file_path",
        "command",
        "pattern",
        "path",
        "url",
        "query",
        "description",
    ]
    .iter()
    .find_map(|k| input.get(k).and_then(Value::as_str))
    .unwrap_or("");
    let target: String = target.chars().take(120).collect();
    if target.is_empty() {
        format!("\n▸ {name}\n")
    } else {
        format!("\n▸ {name} {target}\n")
    }
}

/// Map one stream-json line to events. `Ok(true)` when the turn ended.
pub fn map_line(line: &str, tx: &futures_channel::mpsc::UnboundedSender<Event>) -> bool {
    let v: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return false,
    };
    match v.get("type").and_then(Value::as_str).unwrap_or("") {
        "assistant" => {
            if let Some(blocks) = v.pointer("/message/content").and_then(Value::as_array) {
                for b in blocks {
                    match b.get("type").and_then(Value::as_str) {
                        Some("text") => {
                            let t = b.get("text").and_then(Value::as_str).unwrap_or("");
                            if !t.is_empty() {
                                let _ = tx.unbounded_send(Event::TextDelta {
                                    text: t.to_string(),
                                });
                            }
                        }
                        Some("tool_use") => {
                            let name = b.get("name").and_then(Value::as_str).unwrap_or("tool");
                            let input = b.get("input").cloned().unwrap_or(Value::Null);
                            let _ = tx.unbounded_send(Event::TextDelta {
                                text: tool_line(name, &input),
                            });
                        }
                        _ => {}
                    }
                }
            }
            false
        }
        "user" => {
            // Tool results: only failures are worth a line.
            if let Some(blocks) = v.pointer("/message/content").and_then(Value::as_array) {
                for b in blocks {
                    if b.get("type").and_then(Value::as_str) == Some("tool_result")
                        && b.get("is_error").and_then(Value::as_bool) == Some(true)
                    {
                        let text = match b.get("content") {
                            Some(Value::String(s)) => s.clone(),
                            Some(Value::Array(a)) => a
                                .iter()
                                .filter_map(|c| c.get("text").and_then(Value::as_str))
                                .collect::<Vec<_>>()
                                .join(" "),
                            _ => String::new(),
                        };
                        let text: String = text.chars().take(200).collect();
                        let _ = tx.unbounded_send(Event::TextDelta {
                            text: format!("  ↳ error: {text}\n"),
                        });
                    }
                }
            }
            false
        }
        "result" => {
            let usage = v.get("usage").map(|u| Usage {
                input_tokens: u.get("input_tokens").and_then(Value::as_u64).unwrap_or(0) as u32,
                output_tokens: u.get("output_tokens").and_then(Value::as_u64).unwrap_or(0) as u32,
            });
            if v.get("is_error").and_then(Value::as_bool) == Some(true) {
                let msg = v
                    .get("result")
                    .and_then(Value::as_str)
                    .unwrap_or("claude exited with an error");
                let _ = tx.unbounded_send(Event::Error {
                    message: msg.to_string(),
                });
            } else {
                let stop = match v.get("stop_reason").and_then(Value::as_str) {
                    Some("max_tokens") => StopReason::MaxTokens,
                    Some("end_turn") | None => StopReason::EndTurn,
                    _ => StopReason::Other,
                };
                let _ = tx.unbounded_send(Event::Done {
                    stop,
                    usage: usage.unwrap_or_default(),
                });
            }
            true
        }
        "rate_limit_event" => {
            if v.pointer("/rate_limit_info/status").and_then(Value::as_str) == Some("rejected") {
                let _ = tx.unbounded_send(Event::TextDelta {
                    text: "\n⚠ Claude Code: rate limit reached\n".into(),
                });
            }
            false
        }
        "system" => false,
        other => {
            tracing::debug!("claude-code: skipped a {other:?} line");
            false
        }
    }
}

impl Provider for ClaudeCode {
    fn name(&self) -> String {
        "claude-code".into()
    }

    fn model(&self) -> String {
        if self.model.is_empty() {
            "subscription".into()
        } else {
            self.model.clone()
        }
    }

    fn complete(&self, request: Request) -> EventStream {
        let (tx, rx) = futures_channel::mpsc::unbounded();
        let cwd = request
            .cwd
            .clone()
            .filter(|c| !c.is_empty())
            .or_else(|| {
                std::env::current_dir()
                    .ok()
                    .map(|p| p.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| ".".into());
        let (session, prompt, resume) = self.plan_turn(&cwd, &request.messages);
        let mut args: Vec<String> = vec![
            "-p".into(),
            prompt,
            "--output-format".into(),
            "stream-json".into(),
            "--verbose".into(),
            "--permission-mode".into(),
            self.permission_mode.clone(),
        ];
        if resume {
            args.extend(["--resume".into(), session.clone()]);
        } else {
            args.extend(["--session-id".into(), session.clone()]);
        }
        if !self.allowed_tools.trim().is_empty() {
            args.extend(["--allowedTools".into(), self.allowed_tools.clone()]);
        }
        if let Some(m) = request.model.clone().filter(|m| !m.is_empty()) {
            args.extend(["--model".into(), m]);
        } else if !self.model.is_empty() {
            args.extend(["--model".into(), self.model.clone()]);
        }
        let path = self.path.clone();
        tracing::info!(
            "claude-code: {path} {} (cwd {cwd}, resume {resume})",
            args[2..].join(" ")
        );
        tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, AsyncReadExt};
            let mut child = match tokio::process::Command::new(&path)
                .args(&args)
                .current_dir(&cwd)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
            {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.unbounded_send(Event::Error {
                        message: format!("could not start `{path}`: {e} — is Claude Code installed and logged in (`claude login`)?"),
                    });
                    return;
                }
            };
            let stdout = child.stdout.take().expect("piped");
            let mut stderr = child.stderr.take().expect("piped");
            let mut lines = tokio::io::BufReader::new(stdout).lines();
            let mut ended = false;
            let run = async {
                while let Ok(Some(line)) = lines.next_line().await {
                    if map_line(&line, &tx) {
                        ended = true;
                    }
                }
            };
            let timed_out = tokio::time::timeout(TURN_TIMEOUT, run).await.is_err();
            if timed_out {
                let _ = child.kill().await;
                let _ = tx.unbounded_send(Event::Error {
                    message: "Claude Code did not finish within 10 minutes; the turn was stopped"
                        .into(),
                });
                return;
            }
            let mut err = String::new();
            let _ = stderr.read_to_string(&mut err).await;
            let status = child.wait().await;
            if !ended {
                let tail: String = err
                    .lines()
                    .rev()
                    .take(6)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join("\n");
                let _ = tx.unbounded_send(Event::Error {
                    message: format!(
                        "claude exited ({}) without a result{}{}",
                        status.map(|s| s.to_string()).unwrap_or_default(),
                        if tail.is_empty() { "" } else { ": " },
                        tail
                    ),
                });
            }
        });
        rx
    }

    fn embed(&self, _texts: Vec<String>) -> BoxFuture<Result<Vec<Vec<f32>>, String>> {
        Box::pin(async { Err("Claude Code has no embedding endpoint".into()) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;

    #[test]
    fn maps_the_probed_lines() {
        let (tx, mut rx) = futures_channel::mpsc::unbounded();
        assert!(!map_line(
            r#"{"type":"system","subtype":"init","session_id":"s"}"#,
            &tx
        ));
        assert!(!map_line(
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"ok"},{"type":"tool_use","name":"Read","input":{"file_path":"src/main.rs"}}]}}"#,
            &tx
        ));
        assert!(!map_line(
            r#"{"type":"user","message":{"content":[{"type":"tool_result","is_error":true,"content":"no such file"}]}}"#,
            &tx
        ));
        assert!(map_line(
            r#"{"type":"result","subtype":"success","is_error":false,"stop_reason":"end_turn","usage":{"input_tokens":2,"output_tokens":4}}"#,
            &tx
        ));
        drop(tx);
        let got: Vec<Event> = futures_executor::block_on(async { rx.by_ref().collect().await });
        assert_eq!(got.len(), 4);
        assert_eq!(got[0], Event::TextDelta { text: "ok".into() });
        assert_eq!(
            got[1],
            Event::TextDelta {
                text: "\n▸ Read src/main.rs\n".into()
            }
        );
        assert!(matches!(&got[2], Event::TextDelta { text } if text.contains("no such file")));
        assert!(matches!(
            got[3],
            Event::Done {
                stop: StopReason::EndTurn,
                usage: Usage {
                    input_tokens: 2,
                    output_tokens: 4
                }
            }
        ));
    }

    #[test]
    fn sessions_resume_or_restart() {
        let p = ClaudeCode::new(String::new(), String::new(), String::new(), String::new());
        let m1 = vec![Message::user("hi")];
        let (s1, prompt, resume) = p.plan_turn("/a", &m1);
        assert!(!resume);
        assert_eq!(prompt, "hi");
        let m2 = vec![
            Message::user("hi"),
            Message::assistant(vec![Content::Text {
                text: "hello".into(),
            }]),
            Message::user("more"),
        ];
        let (s2, prompt, resume) = p.plan_turn("/a", &m2);
        assert!(resume);
        assert_eq!(s1, s2);
        assert_eq!(prompt, "more");
        // A cleared chat: one user message again → a new session.
        let (s3, _, resume) = p.plan_turn("/a", &m1);
        assert!(!resume);
        assert_ne!(s3, s1);
        // A restart mid-conversation carries the history.
        let (_, prompt, resume) = p.plan_turn("/b", &m2);
        assert!(!resume);
        assert!(prompt.starts_with("Earlier in this conversation:"));
        assert!(prompt.contains("User: hi") && prompt.contains("Assistant: hello"));
        assert!(prompt.ends_with("Now the user says:\n\nmore"));
    }
}
