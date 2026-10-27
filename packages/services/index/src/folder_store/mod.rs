//! File-backed Sophia cache. Only the index's bounded, ignore-aware code
//! selection enters here. Sophia never parses or authors these files.
pub mod digest;
mod producer;

use moonkale_core::{ContentRef, Node, SourceError};
use sophia_store::{
    folder::source_record, sqlite::Sqlite, FileIngest, FileReport, Store, StoreError,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub const STORE_PATH: &str = ".moonkale/code.sophia";
/// Retry only new owners; an empty or repeated owner is an explicit conflict.
const MAX_OWNER_RETRIES: usize = 16;
/// Bound the aggregate snapshot as well as individual index reads.
pub const MAX_SNAPSHOT_BYTES: usize = 64 * 1024 * 1024;

pub fn normaliser() -> String {
    format!("moonkale-wp66@{}", moonkale_content_id::NORMALIZER_VERSION)
}

/// Text admitted by the index walk, not arbitrary filesystem paths.
#[derive(Clone)]
pub struct CodeFile {
    pub node: Node,
    pub text: String,
}
impl CodeFile {
    pub fn language(&self) -> &str {
        match &self.node.content {
            Some(ContentRef::Text {
                lang: Some(lang), ..
            }) => lang,
            _ => "",
        }
    }
}

/// Per-sync evidence, including genuine no-ops on reopen.
#[derive(Clone, Debug, Default)]
pub struct FolderReport {
    pub written: FileReport,
    pub parsed_files: usize,
    pub skipped_files: usize,
    pub retries: usize,
    pub rebuilt: bool,
    pub refused_definitions: usize,
    pub naming_diagnostics: Vec<String>,
}

/// The handle holds no SQLite connection: a reader can reopen after a cache
/// deletion or a normaliser rebuild. Serialize callers in the index.
pub struct FolderStore {
    root: PathBuf,
}
impl FolderStore {
    pub fn open(root: impl AsRef<Path>) -> Result<Self, SourceError> {
        let root = root.as_ref().canonicalize().map_err(io)?;
        let dir = root.join(".moonkale");
        if dir
            .symlink_metadata()
            .is_ok_and(|m| m.file_type().is_symlink())
        {
            return Err(SourceError::Invalid("cache directory is a symlink".into()));
        }
        std::fs::create_dir_all(&dir).map_err(io)?;
        // Ignore every cache entry, including this ignore file, even outside
        // a git repository. Never modify the user's root .gitignore.
        let ignore = dir.join(".gitignore");
        if !ignore.exists() {
            use std::io::Write;
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(ignore)
                .and_then(|mut f| f.write_all(b"*\n"))
                .map_err(io)?;
        }
        let this = Self { root };
        this.check_path()?;
        Ok(this)
    }
    pub fn path(&self) -> PathBuf {
        self.root.join(STORE_PATH)
    }

    /// Never persist code aliases (including a `.py` link to a hidden secret).
    pub fn admits_path(&self, relative: &str) -> bool {
        if !safe_relative(relative) || relative.split('/').any(|s| s.starts_with('.')) {
            return false;
        }
        let mut path = self.root.clone();
        for part in Path::new(relative).components() {
            path.push(part);
            if path
                .symlink_metadata()
                .is_ok_and(|m| m.file_type().is_symlink())
            {
                return false;
            }
        }
        true
    }

    fn check_path(&self) -> Result<(), SourceError> {
        for p in [
            self.root.join(".moonkale"),
            self.root.join(".moonkale/.gitignore"),
            self.path(),
            self.root.join(".moonkale/code.sophia-wal"),
            self.root.join(".moonkale/code.sophia-shm"),
            self.root.join(".moonkale/code.sophia-journal"),
        ] {
            if p.symlink_metadata()
                .is_ok_and(|m| m.file_type().is_symlink())
            {
                return Err(SourceError::Invalid("cache path is a symlink".into()));
            }
        }
        Ok(())
    }

    fn validate_files(&self, files: &[CodeFile]) -> Result<(), SourceError> {
        let mut total = 0usize;
        for f in files {
            total = total.saturating_add(f.text.len());
            if !matches!(f.language(), "rust" | "python" | "julia")
                || f.text.len() > crate::walk::Limits::default().max_text_bytes as usize
                || !self.admits_path(&f.node.native_key)
                || f.node.native_key.split('/').any(|s| s.starts_with('.'))
                || total > MAX_SNAPSHOT_BYTES
            {
                return Err(SourceError::Invalid("ineligible folder-store input".into()));
            }
        }
        Ok(())
    }

    pub(crate) fn is_broken(&self) -> bool {
        let Ok(db) = Sqlite::open(self.path()) else {
            return true;
        };
        let store = Store::new(db);
        // A readable authored store is protected, not a broken cache.
        match store.is_derived() {
            Ok(false) => false,
            Ok(true) => digest::state(&store).is_err(),
            Err(_) => true,
        }
    }

    /// Rebuild only the reserved derived cache; never follow cache symlinks or
    /// replace an authored store. An unreadable database at this reserved path
    /// is disposable, but a readable non-derived store is not.
    pub fn rebuild(&self, files: &[CodeFile]) -> Result<FolderReport, SourceError> {
        self.check_path()?;
        self.validate_files(files)?;
        if self.path().exists() {
            if let Ok(db) = Sqlite::open(self.path()) {
                if Store::new(db).is_derived().is_ok_and(|derived| !derived) {
                    return Err(error("reserved cache path contains an authored store"));
                }
            }
        }
        for suffix in ["", "-wal", "-shm", "-journal"] {
            match std::fs::remove_file(format!("{}{suffix}", self.path().display())) {
                Ok(()) => (),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                Err(e) => return Err(io(e)),
            }
        }
        let mut report = self.sync(files)?;
        report.rebuilt = true;
        Ok(report)
    }

    /// Compare against a fresh producer snapshot under the folder's cache
    /// directory. Diagnostics contain keys, never record bodies/source text.
    pub fn check(&self, files: &[CodeFile]) -> Result<Vec<String>, SourceError> {
        self.check_path()?;
        self.validate_files(files)?;
        let scratch = tempfile::Builder::new()
            .prefix("check-")
            .tempdir_in(self.root.join(".moonkale"))
            .map_err(io)?;
        // Producer naming reads bounded Cargo/Python metadata from the real
        // root, while its database lives only in the scratch directory.
        let fresh = Self::open(scratch.path())?;
        let mut report = FolderReport::default();
        let changed = files.iter().map(|f| f.node.native_key.clone()).collect();
        let records = producer::produce(&self.root, files, &changed, true, &mut report)?;
        let db = Store::new(Sqlite::open(fresh.path()).map_err(error)?);
        db.set_derived().map_err(error)?;
        db.replace_files(&records.into_values().collect::<Vec<_>>())
            .map_err(error)?;
        let expected = digest::state(&db).map_err(error)?;
        let actual = if self.path().is_file() {
            Sqlite::open(self.path())
                .map_err(error)
                .and_then(|db| digest::state(&Store::new(db)).map_err(error))
        } else {
            Err(error("cache file is missing"))
        };
        let diff = match actual {
            Ok(actual) => digest::differences(&actual, &expected),
            Err(_) => vec!["missing or unreadable active cache records".into()],
        };
        drop(db);
        if !diff.is_empty() {
            self.rebuild(files)?;
            tracing::error!(differences = ?diff, "code graph cache problem; rebuilt from files");
        }
        Ok(diff)
    }

    pub fn sync(&self, files: &[CodeFile]) -> Result<FolderReport, SourceError> {
        self.sync_selected(files, None)
    }

    /// `selected` is a debounce window; `files` is the current eligible
    /// snapshot, used to recover owners on NameClaimed. None reconciles all.
    pub fn sync_selected(
        &self,
        files: &[CodeFile],
        selected: Option<&BTreeSet<String>>,
    ) -> Result<FolderReport, SourceError> {
        self.check_path()?;
        self.validate_files(files)?;
        let existed = self.path().exists();
        let mut store = Store::new(Sqlite::open(self.path()).map_err(error)?);
        if !existed {
            store.set_derived().map_err(error)?;
        } else if !store.is_derived().map_err(error)? {
            return Err(error("reserved cache path contains an authored store"));
        }
        let mut report = FolderReport::default();
        if store
            .meta("folder/normaliser")
            .map_err(error)?
            .is_some_and(|n| n != normaliser().as_bytes())
        {
            drop(store);
            std::fs::remove_file(self.path()).map_err(io)?;
            store = Store::new(Sqlite::open(self.path()).map_err(error)?);
            store.set_derived().map_err(error)?;
            report.rebuilt = true;
        }
        let inputs: BTreeMap<_, _> = files
            .iter()
            .map(|f| (f.node.native_key.clone(), f))
            .collect();
        let old = store.files().map_err(error)?;
        let mut changed = BTreeSet::new();
        for (path, file) in &inputs {
            let same = store
                .file_manifest(path)
                .map_err(error)?
                .is_some_and(|m| m.source == Some(source_record(path, &file.text).id));
            if same {
                report.skipped_files += 1;
            }
            if !same && (report.rebuilt || selected.is_none_or(|s| s.contains(path))) {
                changed.insert(path.clone());
            }
        }
        for p in old {
            if !inputs.contains_key(&p) && selected.is_none_or(|s| s.contains(&p)) {
                changed.insert(p);
            }
        }
        if changed.is_empty() {
            return Ok(report);
        }
        // Julia's unit is a module spread over includes. Recompute its
        // dependants together; the producer compares manifests before write.
        let julia_changed = changed.iter().any(|p| p.ends_with(".jl"));
        let mut produced =
            producer::produce(&self.root, files, &changed, julia_changed, &mut report)?;
        if julia_changed {
            for (p, f) in &produced {
                if p.ends_with(".jl") {
                    let new = manifest(f);
                    let old = store.file_manifest(p).map_err(error)?;
                    let same_source = old.as_ref().is_some_and(|m| m.source == new.source);
                    if old.as_ref() != Some(&new) && (selected.is_none() || same_source) {
                        changed.insert(p.clone());
                    }
                }
            }
        }
        let mut batch: BTreeMap<String, FileIngest> = changed
            .iter()
            .map(|p| (p.clone(), produced.remove(p).unwrap_or_else(|| deleted(p))))
            .collect();
        loop {
            let values: Vec<_> = batch.values().cloned().collect();
            match store.replace_files(&values) {
                Ok(r) => {
                    report.written = r;
                    return Ok(report);
                }
                Err(StoreError::NameClaimed { name, owner }) => {
                    if owner.is_empty()
                        || batch.contains_key(&owner)
                        || report.retries >= MAX_OWNER_RETRIES
                    {
                        return Err(error(StoreError::NameClaimed { name, owner }));
                    }
                    report.retries += 1;
                    let more = BTreeSet::from([owner.clone()]);
                    let mut extra = producer::produce(
                        &self.root,
                        files,
                        &more,
                        owner.ends_with(".jl"),
                        &mut report,
                    )?;
                    batch.insert(
                        owner.clone(),
                        extra.remove(&owner).unwrap_or_else(|| deleted(&owner)),
                    );
                }
                Err(e) => return Err(error(e)),
            }
        }
    }
}

fn manifest(f: &FileIngest) -> sophia_store::FileManifest {
    let mut facts: Vec<_> = f.facts.iter().map(|x| x.id).collect();
    facts.sort();
    facts.dedup();
    sophia_store::FileManifest {
        path: f.path.clone(),
        normaliser: f.normaliser.clone(),
        source: f.text.as_ref().map(|s| source_record(&f.path, s).id),
        names: f
            .names
            .iter()
            .map(|(n, i)| (n.clone(), f.records[*i].id))
            .collect(),
        generics: f
            .generics
            .iter()
            .map(|g| {
                (
                    g.path.clone(),
                    g.lang.clone(),
                    g.members.iter().map(|i| f.records[*i].id).collect(),
                )
            })
            .collect(),
        spans: f
            .spans
            .iter()
            .map(|(i, a, b)| (f.records[*i].id, *a, *b))
            .collect(),
        facts,
    }
}
fn deleted(path: &str) -> FileIngest {
    FileIngest {
        path: path.into(),
        normaliser: normaliser(),
        ..Default::default()
    }
}
pub(crate) fn safe_relative(p: &str) -> bool {
    !p.is_empty()
        && !p.contains('\\')
        && Path::new(p)
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
}
fn io(e: std::io::Error) -> SourceError {
    SourceError::Io(e.to_string())
}
pub(crate) fn error(e: impl std::fmt::Display) -> SourceError {
    SourceError::Invalid(e.to_string())
}
