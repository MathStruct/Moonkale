//! `LlmToolContribution` — expose a command to LLM agents as a tool.
//!
//! Any command with an `ArgSchema` can be flagged `llm_tool = true` and gets
//! a generated tool definition. The extension may additionally provide a
//! `description_for_model` and a `risk` level (`ReadOnly | Mutating |
//! Destructive`) that `moonkale-llm::policy` uses to decide whether to ask
//! the user before running it.
