//! Change notifications flowing *from* a source (Milestone 16).
//!
//! A source that can see changes made outside Moonkale (a folder on disk)
//! keeps a numbered log of the relative paths that changed. Consumers poll
//! it with [`Source::changes_since`](super::Source::changes_since), a long
//! poll: the call returns as soon as something newer than `since` exists,
//! or after a while with nothing new. The log says *which paths* changed,
//! not how — watchers disagree about create/modify/rename across platforms
//! and editors (an atomic save is a rename over the file) — so consumers
//! ask the source what each path is now.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Changes {
    /// The log position after these changes; pass it as the next `since`.
    pub seq: u64,
    /// Relative paths (`/`-separated, as in `Node::native_key`) that changed
    /// after `since`, oldest first, without duplicates. Empty when the poll
    /// timed out with nothing new.
    pub paths: Vec<String>,
    /// Too much changed to list, or the log no longer reaches back to
    /// `since` (a `git checkout`, a restarted watcher): re-read everything.
    pub reset: bool,
}
