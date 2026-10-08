//! # moonkale-typst
//!
//! Compile a Typst document to SVG pages, in process. The `World` reads the
//! main source from memory and other files (imports, images) from a root
//! directory; fonts are the ones embedded in `typst-assets`, so nothing on
//! the machine is required. Packages (`@preview/…`) are not fetched yet.
//!
//! Runs natively only: desktop compiles in-process, the server compiles for
//! web clients (`moonkale_server::compile_typst`).

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use typst::diag::{FileError, FileResult, SourceDiagnostic};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_layout::PagedDocument;

struct Fonts {
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
}

fn fonts() -> &'static Fonts {
    static FONTS: OnceLock<Fonts> = OnceLock::new();
    FONTS.get_or_init(|| {
        let mut book = FontBook::new();
        let mut fonts = Vec::new();
        for data in typst_assets::fonts() {
            for font in Font::iter(Bytes::new(data)) {
                book.push(font.info().clone());
                fonts.push(font);
            }
        }
        Fonts {
            book: LazyHash::new(book),
            fonts,
        }
    })
}

fn library() -> &'static LazyHash<Library> {
    static LIB: OnceLock<LazyHash<Library>> = OnceLock::new();
    LIB.get_or_init(|| LazyHash::new(Library::default()))
}

/// A world whose main file is `text` at `main_rel` (a `/`-rooted virtual
/// path such as `/notes/report.typ`) and whose other files live under
/// `root` on disk.
pub struct MoonkaleWorld {
    root: PathBuf,
    main: FileId,
    source: Source,
}

impl MoonkaleWorld {
    pub fn new(root: impl Into<PathBuf>, main_rel: &str, text: String) -> Result<Self, String> {
        let vpath = VirtualPath::new(main_rel).map_err(|e| e.to_string())?;
        let main = FileId::new(RootedPath::new(VirtualRoot::Project, vpath));
        Ok(Self {
            root: root.into(),
            main,
            source: Source::new(main, text),
        })
    }

    fn resolve(&self, id: FileId) -> FileResult<PathBuf> {
        match id.root() {
            VirtualRoot::Project => {
                let rel = id.vpath().get_without_slash();
                let path = self.root.join(rel);
                if !path.starts_with(&self.root) {
                    return Err(FileError::AccessDenied);
                }
                Ok(path)
            }
            VirtualRoot::Package(spec) => Err(FileError::Other(Some(
                format!("packages are not supported yet ({spec})").into(),
            ))),
        }
    }
}

impl World for MoonkaleWorld {
    fn library(&self) -> &LazyHash<Library> {
        library()
    }
    fn book(&self) -> &LazyHash<FontBook> {
        &fonts().book
    }
    fn main(&self) -> FileId {
        self.main
    }
    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main {
            return Ok(self.source.clone());
        }
        let path = self.resolve(id)?;
        let text = std::fs::read_to_string(&path).map_err(|e| FileError::from_io(e, &path))?;
        Ok(Source::new(id, text))
    }
    fn file(&self, id: FileId) -> FileResult<Bytes> {
        if id == self.main {
            return Ok(Bytes::from_string(self.source.text().to_string()));
        }
        let path = self.resolve(id)?;
        std::fs::read(&path)
            .map(Bytes::new)
            .map_err(|e| FileError::from_io(e, &path))
    }
    fn font(&self, index: usize) -> Option<Font> {
        fonts().fonts.get(index).cloned()
    }
    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        None
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub message: String,
    pub hint: Option<String>,
}

/// Compile `text` (a Typst file at `main_rel` under `root`) to one SVG
/// string per page. Errors come back as diagnostics, never as a panic.
pub fn compile_to_svg(
    root: &Path,
    main_rel: &str,
    text: String,
) -> Result<Vec<String>, Vec<Diagnostic>> {
    let world = MoonkaleWorld::new(root, main_rel, text).map_err(|m| {
        vec![Diagnostic {
            message: m,
            hint: None,
        }]
    })?;
    let result = typst::compile::<PagedDocument>(&world);
    match result.output {
        Ok(doc) => Ok(doc
            .pages()
            .iter()
            .map(|p| typst_svg::svg(p, &Default::default()))
            .collect()),
        Err(errors) => Err(errors.iter().map(to_diag).collect()),
    }
}

fn to_diag(d: &SourceDiagnostic) -> Diagnostic {
    Diagnostic {
        message: d.message.to_string(),
        hint: d.hints.first().map(|h| h.v.to_string()),
    }
}

/// How many compiles run at once, and how long a caller waits for one.
pub const MAX_CONCURRENT: usize = 2;
pub const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// [`compile_to_svg`] with a budget (#13): on a blocking thread (it never
/// holds up the async runtime), at most [`MAX_CONCURRENT`] at a time, and the
/// caller gets an error after [`TIMEOUT`]. Typst cannot be interrupted, so a
/// runaway compile finishes on its thread — but keeps its slot, so a flood
/// of them cannot use more than [`MAX_CONCURRENT`] threads. Needs a tokio
/// runtime.
pub async fn compile_bounded(
    root: std::path::PathBuf,
    main_rel: String,
    text: String,
) -> Result<Vec<String>, Vec<Diagnostic>> {
    static SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(MAX_CONCURRENT);
    let fail = |message: String| {
        vec![Diagnostic {
            message,
            hint: None,
        }]
    };
    let work = async {
        let permit = SLOTS.acquire().await.map_err(|e| fail(e.to_string()))?;
        let handle = tokio::task::spawn_blocking(move || {
            let out = compile_to_svg(&root, &main_rel, text);
            drop(permit);
            out
        });
        handle.await.map_err(|e| fail(e.to_string()))?
    };
    match tokio::time::timeout(TIMEOUT, work).await {
        Ok(out) => out,
        Err(_) => Err(fail(format!(
            "Typst did not finish within {} s",
            TIMEOUT.as_secs()
        ))),
    }
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn bounded_compile_answers() {
        let dir = tempfile::tempdir().unwrap();
        let out =
            super::compile_bounded(dir.path().into(), "/main.typ".into(), "= Hi".into()).await;
        assert_eq!(out.map(|p| p.len()).ok(), Some(1));
    }

    use super::*;

    #[test]
    fn compiles_a_document_to_svg_pages() {
        let dir = std::env::temp_dir();
        let pages = compile_to_svg(
            &dir,
            "/main.typ",
            "= Hello\nMoonkale $x^2$\n#pagebreak()\nsecond".into(),
        )
        .unwrap();
        assert_eq!(pages.len(), 2);
        assert!(pages[0].starts_with("<svg"));
        assert!(pages[0].contains("<path") || pages[0].contains("<use"));
    }

    #[test]
    fn errors_are_diagnostics() {
        let err = compile_to_svg(
            std::env::temp_dir().as_path(),
            "/main.typ",
            "#let x = (".into(),
        )
        .unwrap_err();
        assert!(!err.is_empty());
        assert!(!err[0].message.is_empty());
    }
}
