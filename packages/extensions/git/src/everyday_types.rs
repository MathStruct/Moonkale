//! Typed everyday operations. Clients supply identities, never Git arguments.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EverydayAction {
    CreateBranch { name: String, commit: String },
    SwitchBranch { name: String, stash: bool },
    RenameBranch { name: String, to: String },
    DeleteBranch { name: String },
    Upstream { name: String, upstream: String },
    Revert { commit: String },
    Reset { commit: String, mode: ResetMode },
    CherryPick { commit: String },
    Restore { commit: String, name: String },
    StashPush { message: String, untracked: bool },
    StashApply { oid: String },
    StashPop { oid: String },
    StashDrop { oid: String },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResetMode {
    Soft,
    Mixed,
    Hard,
}

/// Review state includes repository identity, refs, index/worktree diff and status.
/// It is regenerated on confirmation; stale requests do not execute.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EverydayPlan {
    pub repository: String,
    pub head: String,
    pub refs: String,
    pub status: String,
    pub diff: String,
    pub summary: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefEntry {
    pub name: String,
    pub oid: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryEntry {
    pub oid: String,
    pub selector: String,
    pub subject: String,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EverydayState {
    pub branches: Vec<RefEntry>,
    pub upstreams: Vec<String>,
    pub stashes: Vec<RecoveryEntry>,
    pub reflog: Vec<RecoveryEntry>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlameLine {
    pub line: u32,
    pub commit: String,
    pub author: String,
    pub summary: String,
    pub previous: Option<String>,
    pub previous_path: Option<String>,
}
