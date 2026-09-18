//! The agent loop: user text → model turns → tool calls through the policy
//! gate and the host → until the model stops. Dioxus-free; the panel drives
//! it and renders the events.

use crate::audit::{summarize, AuditEntry, AuditLog};
use crate::policy::{Class, Decision, Policy};
use crate::provider::Provider;
use crate::tools::{builtin_tools, cap, ToolCall};
use crate::types::{Content, Event, Message, Request, StopReason, ToolDef, Usage};
use futures_util::StreamExt;
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Host futures run on the UI thread (Dioxus `spawn`), so they need not be `Send`.
pub type HostFuture<T> = Pin<Box<dyn Future<Output = T>>>;

/// What the agent needs from the application.
pub trait ToolHost {
    /// Run an allowed call; `Err` is reported to the model as a tool error.
    fn call(&self, call: ToolCall) -> HostFuture<Result<String, String>>;
    /// Ask the user whether a `Mutating`/`Destructive` call may run.
    fn approve(&self, call: ToolCall, class: Class) -> HostFuture<bool>;
}

#[derive(Clone, Debug, PartialEq)]
pub enum ToolOutcome {
    Ran { ok: bool },
    Denied,
    Declined,
}

/// Progress, in order, for the panel.
#[derive(Clone, Debug, PartialEq)]
pub enum AgentEvent {
    /// Streamed assistant text.
    TextDelta(String),
    /// The model asked for a tool; policy has decided.
    ToolCall {
        id: String,
        name: String,
        input: Value,
        class: Class,
        decision: Decision,
    },
    /// A tool finished (or was refused); `summary` is what the panel shows.
    ToolResult {
        id: String,
        outcome: ToolOutcome,
        summary: String,
    },
    /// One model turn ended.
    TurnDone {
        stop: StopReason,
        usage: Usage,
    },
    /// The whole exchange ended (no more tool calls).
    Finished,
    Error(String),
}

pub struct Agent {
    pub provider: Arc<dyn Provider>,
    pub policy: Policy,
    pub system: String,
    pub tools: Vec<ToolDef>,
    pub messages: Vec<Message>,
    pub audit: AuditLog,
    /// Safety valve against a model looping on tools.
    pub max_tool_rounds: usize,
    pub max_tokens: u32,
}

impl Agent {
    pub fn new(provider: Arc<dyn Provider>, system: String) -> Self {
        Self {
            provider,
            policy: Policy::default(),
            system,
            tools: builtin_tools(),
            messages: Vec::new(),
            audit: AuditLog::default(),
            max_tool_rounds: 12,
            max_tokens: 2048,
        }
    }

    /// Send one user message and run turns until the model stops. Events go
    /// to `on_event` as they happen; the transcript stays in `self.messages`.
    pub async fn send(
        &mut self,
        user_text: String,
        host: &dyn ToolHost,
        on_event: &mut dyn FnMut(AgentEvent),
    ) {
        self.messages.push(Message::user(user_text));
        for _round in 0..=self.max_tool_rounds {
            let request = Request {
                model: None,
                system: self.system.clone(),
                messages: self.messages.clone(),
                tools: self.tools.clone(),
                max_tokens: self.max_tokens,
            };
            let mut stream = self.provider.complete(request);
            let mut text = String::new();
            let mut calls: Vec<ToolCall> = Vec::new();
            let mut stop = StopReason::Other;
            let mut usage = Usage::default();
            let mut failed = false;
            while let Some(ev) = stream.next().await {
                match ev {
                    Event::TextDelta { text: t } => {
                        text.push_str(&t);
                        on_event(AgentEvent::TextDelta(t));
                    }
                    Event::ToolUse { id, name, input } => calls.push(ToolCall { id, name, input }),
                    Event::Done { stop: s, usage: u } => {
                        stop = s;
                        usage = u;
                        break;
                    }
                    Event::Error { message } => {
                        on_event(AgentEvent::Error(message));
                        failed = true;
                        break;
                    }
                }
            }
            let mut content: Vec<Content> = Vec::new();
            if !text.is_empty() {
                content.push(Content::Text { text });
            }
            for c in &calls {
                content.push(Content::ToolUse {
                    id: c.id.clone(),
                    name: c.name.clone(),
                    input: c.input.clone(),
                });
            }
            if !content.is_empty() {
                self.messages.push(Message::assistant(content));
            }
            if failed {
                // Keep the transcript consistent for the next attempt: the
                // model never answered, so drop nothing more.
                return;
            }
            on_event(AgentEvent::TurnDone { stop, usage });
            if calls.is_empty() {
                on_event(AgentEvent::Finished);
                return;
            }
            let mut results = Vec::new();
            for call in calls {
                let (class, decision) = self.policy.decide(&call);
                on_event(AgentEvent::ToolCall {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    input: call.input.clone(),
                    class,
                    decision,
                });
                let started = now_ms();
                let (outcome, output, approved) = match decision {
                    Decision::Deny => (
                        ToolOutcome::Denied,
                        format!("Tool {} is not allowed by policy.", call.name),
                        None,
                    ),
                    Decision::Ask if !host.approve(call.clone(), class).await => (
                        ToolOutcome::Declined,
                        "The user declined this action.".to_string(),
                        Some(false),
                    ),
                    d => {
                        let approved = (d == Decision::Ask).then_some(true);
                        match host.call(call.clone()).await {
                            Ok(out) => (ToolOutcome::Ran { ok: true }, cap(out), approved),
                            Err(e) => (
                                ToolOutcome::Ran { ok: false },
                                format!("error: {e}"),
                                approved,
                            ),
                        }
                    }
                };
                let ok = matches!(outcome, ToolOutcome::Ran { ok: true });
                let summary = summarize(&output);
                self.audit.push(AuditEntry {
                    seq: 0,
                    tool: call.name.clone(),
                    input: call.input.clone(),
                    class,
                    decision,
                    approved,
                    ok,
                    summary: summary.clone(),
                    millis: now_ms().saturating_sub(started),
                });
                on_event(AgentEvent::ToolResult {
                    id: call.id.clone(),
                    outcome,
                    summary,
                });
                results.push(Content::ToolResult {
                    id: call.id,
                    output,
                    is_error: !ok,
                });
            }
            self.messages.push(Message::tool_results(results));
        }
        on_event(AgentEvent::Error(format!(
            "stopped after {} tool rounds",
            self.max_tool_rounds
        )));
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn now_ms() -> u64 {
    use std::sync::OnceLock;
    use std::time::Instant;
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}
#[cfg(target_arch = "wasm32")]
fn now_ms() -> u64 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockProvider;
    use serde_json::json;
    use std::cell::RefCell;

    struct Host {
        calls: RefCell<Vec<String>>,
        approve: bool,
    }
    impl ToolHost for Host {
        fn call(&self, call: ToolCall) -> HostFuture<Result<String, String>> {
            self.calls.borrow_mut().push(call.name.clone());
            Box::pin(async move { Ok(format!("result of {}", call.name)) })
        }
        fn approve(&self, _call: ToolCall, _class: Class) -> HostFuture<bool> {
            let a = self.approve;
            Box::pin(async move { a })
        }
    }

    #[tokio::test]
    async fn runs_a_tool_round_and_finishes() {
        let mut agent = Agent::new(Arc::new(MockProvider::scripted()), "sys".into());
        let host = Host {
            calls: RefCell::new(Vec::new()),
            approve: true,
        };
        let mut events = Vec::new();
        agent
            .send(
                "/tool index.search {\"query\":\"x\"}".into(),
                &host,
                &mut |e| events.push(e),
            )
            .await;
        assert_eq!(host.calls.borrow().as_slice(), ["index.search"]);
        assert!(events.iter().any(|e| matches!(e, AgentEvent::ToolCall { name, decision: Decision::Allow, .. } if name == "index.search")));
        assert!(matches!(events.last(), Some(AgentEvent::Finished)));
        // user, assistant(tool_use), user(tool_result), assistant(text)
        assert_eq!(agent.messages.len(), 4);
        assert!(agent.messages[3]
            .text()
            .starts_with("Tool `index.search` returned: result of index.search"));
        assert_eq!(agent.audit.entries.len(), 1);
    }

    #[tokio::test]
    async fn declined_write_is_reported_to_the_model() {
        let mut agent = Agent::new(Arc::new(MockProvider::scripted()), "sys".into());
        let host = Host {
            calls: RefCell::new(Vec::new()),
            approve: false,
        };
        let mut events = Vec::new();
        agent
            .send(
                "/tool source.text_query {\"source\":\"s\",\"dialect\":\"sql\",\"text\":\"DELETE FROM t\"}".into(),
                &host,
                &mut |e| events.push(e),
            )
            .await;
        assert!(host.calls.borrow().is_empty());
        assert!(events.iter().any(|e| matches!(
            e,
            AgentEvent::ToolResult {
                outcome: ToolOutcome::Declined,
                ..
            }
        )));
        assert_eq!(agent.audit.entries[0].approved, Some(false));
        let _ = json!({});
    }
}
