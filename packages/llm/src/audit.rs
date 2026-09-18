//! Append-only log of every tool call: what was asked, how policy decided,
//! what came back (summarised), how long it took. Shown in the agent panel.

use crate::policy::{Class, Decision};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AuditEntry {
    pub seq: u64,
    pub tool: String,
    pub input: Value,
    pub class: Class,
    pub decision: Decision,
    /// `true` when the user approved an `Ask`.
    pub approved: Option<bool>,
    pub ok: bool,
    /// First line(s) of the result or the error.
    pub summary: String,
    pub millis: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AuditLog {
    pub entries: Vec<AuditEntry>,
}

impl AuditLog {
    pub fn push(&mut self, mut entry: AuditEntry) -> &AuditEntry {
        entry.seq = self.entries.len() as u64 + 1;
        self.entries.push(entry);
        self.entries.last().unwrap()
    }
}

/// First 160 characters on one line.
pub fn summarize(text: &str) -> String {
    let one: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() > 160 {
        format!("{}…", one.chars().take(160).collect::<String>())
    } else {
        one
    }
}
