//! The JSON shapes crossing the guest/host boundary. Shared by the host
//! and by extensions written in Rust (they can depend on this crate with no
//! features to get the types).

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const ABI_VERSION: u32 = 1;

/// What `manifest()` returns.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WasmManifest {
    pub abi: u32,
    /// Reverse-DNS id, must not collide with built-ins.
    pub id: String,
    pub name: String,
    pub description: String,
    /// `"read-sources"`, `"write-files"`, `"network"` … (see `Manifest`).
    pub permissions: Vec<String>,
    pub commands: Vec<WasmCommand>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WasmCommand {
    /// `"<ext>.<name>"`, used as the tool name.
    pub id: String,
    pub title: String,
    pub description: String,
    /// JSON Schema for the arguments; `{}` when none.
    pub input_schema: Value,
    /// Offer it to the agent as a tool.
    pub llm_tool: bool,
}

/// What `run()` receives.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RunRequest {
    pub command: String,
    #[serde(default)]
    pub args: Value,
}

/// What `run()` returns.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RunReply {
    Ok { ok: bool, result: String },
    Err { ok: bool, error: String },
}

impl RunReply {
    pub fn into_result(self) -> Result<String, String> {
        match self {
            RunReply::Ok { ok: true, result } => Ok(result),
            RunReply::Ok { result, .. } => Err(result),
            RunReply::Err { error, .. } => Err(error),
        }
    }
}

/// A host call from the guest (`call(json)`), checked against permissions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum HostCall {
    /// `read-sources`
    ListSources,
    /// `read-sources`: `query` is a `moonkale_core::Query` as JSON.
    Query { source: String, query: Value },
    /// `read-sources`
    FetchText { source: String, node: String },
}

impl HostCall {
    pub fn permission(&self) -> &'static str {
        match self {
            HostCall::ListSources | HostCall::Query { .. } | HostCall::FetchText { .. } => {
                "read-sources"
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HostReply {
    Ok { ok: bool, result: Value },
    Err { ok: bool, error: String },
}

impl HostReply {
    pub fn ok(result: Value) -> Self {
        HostReply::Ok { ok: true, result }
    }
    pub fn err(error: impl Into<String>) -> Self {
        HostReply::Err {
            ok: false,
            error: error.into(),
        }
    }
}
