//! Source openers — "this file or directory opens as that kind of source".
//!
//! Until Milestone 18 the decision was written out four times (the desktop,
//! the server, the phone and the Explorer each asked `is_sqlite_path`,
//! `is_rocksdb_path` … in their own order). Now each driver crate offers its
//! [`SourceOpener`]s, the app puts them into one [`Openers`] list, and
//! everything asks that list: the Explorer (which entries are databases) on
//! every target, the desktop and the server (open one) where the drivers are
//! built.
//!
//! An opener has two halves: [`SourceOpener::matches`] looks at the name only
//! — cheap, no I/O, compiled everywhere, so the web client and the phone know
//! a database when they see one — and [`SourceOpener::open`], which is `None`
//! in builds without that driver.

use crate::{Source, SourceError};
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

/// What [`SourceOpener::open`] returns.
#[cfg(not(target_arch = "wasm32"))]
pub type OpenFuture = Pin<Box<dyn Future<Output = Result<Arc<dyn Source>, SourceError>> + Send>>;
/// What [`SourceOpener::open`] returns.
#[cfg(target_arch = "wasm32")]
pub type OpenFuture = Pin<Box<dyn Future<Output = Result<Arc<dyn Source>, SourceError>>>>;

/// Whether an opener applies to files, directories or both.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    File,
    Dir,
    Either,
}

/// One kind of openable source, contributed by a driver crate.
#[derive(Clone, Copy)]
pub struct SourceOpener {
    /// Stable id (`"sqlite"`, `"rocksdb"`).
    pub id: &'static str,
    /// What the UI calls it (`"SQLite database"`).
    pub name: &'static str,
    pub shape: Shape,
    /// Whether a path (as the folder source names it, `/`-separated) is one,
    /// by its name only.
    pub matches: fn(&str) -> bool,
    /// Open it. `None` where this build has no driver (the web client, the
    /// phone, a build without the feature); such paths still show as
    /// databases and are opened by the server.
    pub open: Option<fn(PathBuf) -> OpenFuture>,
}

impl SourceOpener {
    /// Whether this opener claims `path`, given whether it is a directory.
    pub fn applies(&self, path: &str, is_dir: bool) -> bool {
        let shape_ok = match self.shape {
            Shape::File => !is_dir,
            Shape::Dir => is_dir,
            Shape::Either => true,
        };
        shape_ok && (self.matches)(path)
    }
}

impl std::fmt::Debug for SourceOpener {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourceOpener")
            .field("id", &self.id)
            .field("shape", &self.shape)
            .field("can_open", &self.open.is_some())
            .finish()
    }
}

/// An empty list, for configurations without database sources (tests).
pub static NO_OPENERS: Openers = Openers::none();

/// The openers of a build, in priority order (the first that applies wins).
#[derive(Debug, Default)]
pub struct Openers(Vec<SourceOpener>);

impl Openers {
    /// No openers: every path is a plain file or folder.
    pub const fn none() -> Self {
        Self(Vec::new())
    }

    pub fn new(openers: Vec<SourceOpener>) -> Self {
        Self(openers)
    }

    pub fn all(&self) -> &[SourceOpener] {
        &self.0
    }

    /// The opener for `path`, if any.
    pub fn find(&self, path: &str, is_dir: bool) -> Option<&SourceOpener> {
        self.0.iter().find(|o| o.applies(path, is_dir))
    }

    /// Whether `path` opens as a source other than a folder.
    pub fn is_database(&self, path: &str, is_dir: bool) -> bool {
        self.find(path, is_dir).is_some()
    }

    /// Open `path` with the first opener that applies and can open it.
    /// `Ok(None)`: no opener claims it (open it as a folder or a file).
    /// `Err(Unsupported)`: an opener claims it but this build has no driver.
    pub async fn open(
        &self,
        path: PathBuf,
        is_dir: bool,
    ) -> Result<Option<Arc<dyn Source>>, SourceError> {
        let name = path.to_string_lossy().replace('\\', "/");
        let Some(opener) = self.find(&name, is_dir) else {
            return Ok(None);
        };
        match opener.open {
            Some(open) => open(path).await.map(Some),
            None => Err(SourceError::Unsupported(format!(
                "{} (this build has no driver for it)",
                opener.name
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_x(p: &str) -> bool {
        p.ends_with(".x")
    }
    fn is_y(p: &str) -> bool {
        p.ends_with(".y") || p.ends_with(".x")
    }

    #[test]
    fn first_applicable_opener_wins_and_shape_counts() {
        let list = Openers::new(vec![
            SourceOpener {
                id: "x",
                name: "X",
                shape: Shape::Dir,
                matches: is_x,
                open: None,
            },
            SourceOpener {
                id: "y",
                name: "Y",
                shape: Shape::File,
                matches: is_y,
                open: None,
            },
        ]);
        assert_eq!(list.find("a.x", true).map(|o| o.id), Some("x"));
        assert_eq!(list.find("a.x", false).map(|o| o.id), Some("y"));
        assert_eq!(list.find("a.y", true).map(|o| o.id), None);
        assert!(!list.is_database("a.z", false));
        assert!(Openers::none().find("a.x", true).is_none());
    }
}
