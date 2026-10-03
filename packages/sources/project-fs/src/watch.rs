//! Watching a folder for changes made outside Moonkale (Milestone 16).
//!
//! A [`FolderWatch`] keeps a bounded, numbered log of the relative paths
//! that changed; [`FolderWatch::changes_since`] is the long poll behind
//! `Source::changes_since`. Native only (`notify`: inotify, FSEvents,
//! ReadDirectoryChangesW); the crate is an empty shell on wasm32.
//!
//! What is watched is what the Explorer shows: every directory the
//! `.gitignore`-aware walk visits, one non-recursive watch each — never
//! `target/`, `node_modules/` or `.git/`, which on a Rust workspace are
//! millions of files and would exhaust the inotify limit. Directories that
//! appear later are added as they are created. Events for hidden paths, for
//! paths the root `.gitignore` excludes and for Moonkale's own temporary
//! files are dropped; access events are ignored.

#[cfg(not(target_arch = "wasm32"))]
pub use native::FolderWatch;

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use ignore::gitignore::{Gitignore, GitignoreBuilder};
    use ignore::WalkBuilder;
    use moonkale_core::Changes;
    use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
    use std::collections::{HashSet, VecDeque};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{mpsc, Arc, Mutex};
    use std::time::Duration;

    /// Entries kept in the log; a client further behind gets `reset`.
    const LOG_CAP: usize = 4096;
    /// More distinct paths than this in one answer becomes `reset`.
    const BURST: usize = 512;
    /// After the first change arrives, wait this long for the rest of the
    /// burst (an editor's save is several events) before answering.
    const SETTLE: Duration = Duration::from_millis(120);

    pub struct FolderWatch {
        shared: Arc<Shared>,
        stop: Arc<AtomicBool>,
    }

    struct Shared {
        log: Mutex<Log>,
        wake: tokio::sync::Notify,
    }

    struct Log {
        /// Position of the newest entry (or the base when empty).
        seq: u64,
        /// Position before the oldest entry still kept: a client at or
        /// after it can be answered exactly.
        floor: u64,
        entries: VecDeque<(u64, String)>,
    }

    impl FolderWatch {
        /// Start watching `root` (canonical). Walks the visible directories
        /// once, so call it off the async executor (`spawn_blocking`).
        pub fn start(root: &Path) -> std::io::Result<Self> {
            let root = root.to_path_buf();
            // A per-start base, so a client still holding a position from an
            // earlier watcher (a restarted server) is told to reset rather
            // than being answered from an unrelated log.
            let base = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64 * 1000)
                .unwrap_or(1);
            let shared = Arc::new(Shared {
                log: Mutex::new(Log {
                    seq: base,
                    floor: base,
                    entries: VecDeque::new(),
                }),
                wake: tokio::sync::Notify::new(),
            });
            let (tx, rx) = mpsc::channel();
            let mut watcher = notify::recommended_watcher(tx).map_err(to_io)?;
            let mut watched = HashSet::new();
            watch_tree(&mut watcher, &mut watched, &root, &root)?;
            let stop = Arc::new(AtomicBool::new(false));
            let filter = Filter::new(&root);
            let thread_shared = shared.clone();
            let thread_stop = stop.clone();
            std::thread::Builder::new()
                .name("moonkale-watch".into())
                .spawn(move || {
                    run(
                        watcher,
                        watched,
                        rx,
                        root,
                        filter,
                        thread_shared,
                        thread_stop,
                    )
                })?;
            Ok(Self { shared, stop })
        }

        /// The current log position (nothing to report before it).
        pub fn position(&self) -> u64 {
            self.shared.log.lock().unwrap().seq
        }

        /// Paths changed after `since`, waiting up to `wait` for the first
        /// one. `since == 0` means "from now": the answer is the current
        /// position with no paths, immediately.
        pub async fn changes_since(&self, since: u64, wait: Duration) -> Changes {
            if since == 0 {
                return Changes {
                    seq: self.position(),
                    ..Changes::default()
                };
            }
            let notified = self.shared.wake.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.shared.log.lock().unwrap().seq == since {
                if tokio::time::timeout(wait, notified).await.is_err() {
                    return Changes {
                        seq: since,
                        ..Changes::default()
                    };
                }
                tokio::time::sleep(SETTLE).await;
            }
            self.answer(since)
        }

        fn answer(&self, since: u64) -> Changes {
            let log = self.shared.log.lock().unwrap();
            if since < log.floor || since > log.seq {
                return Changes {
                    seq: log.seq,
                    paths: Vec::new(),
                    reset: true,
                };
            }
            let mut seen = HashSet::new();
            let mut paths = Vec::new();
            for (s, p) in log.entries.iter() {
                if *s > since && seen.insert(p.as_str()) {
                    paths.push(p.clone());
                }
            }
            if paths.len() > BURST {
                return Changes {
                    seq: log.seq,
                    paths: Vec::new(),
                    reset: true,
                };
            }
            Changes {
                seq: log.seq,
                paths,
                reset: false,
            }
        }
    }

    impl Drop for FolderWatch {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
        }
    }

    fn to_io(e: notify::Error) -> std::io::Error {
        std::io::Error::other(e.to_string())
    }

    /// One non-recursive watch per visible directory under `dir`.
    fn watch_tree(
        watcher: &mut RecommendedWatcher,
        watched: &mut HashSet<PathBuf>,
        root: &Path,
        dir: &Path,
    ) -> std::io::Result<()> {
        // Rooted at `root` so parent .gitignore files apply, pruned to `dir`.
        let walk = WalkBuilder::new(root)
            .hidden(true)
            .git_ignore(true)
            .require_git(false)
            .git_exclude(true)
            .parents(true)
            .filter_entry({
                let dir = dir.to_path_buf();
                move |e| e.path().starts_with(&dir) || dir.starts_with(e.path())
            })
            .build();
        for entry in walk.flatten() {
            let path = entry.path();
            if !entry.file_type().is_some_and(|t| t.is_dir()) || !path.starts_with(dir) {
                continue;
            }
            if watched.insert(path.to_path_buf()) {
                watcher
                    .watch(path, RecursiveMode::NonRecursive)
                    .map_err(to_io)?;
            }
        }
        Ok(())
    }

    /// Hidden components, Moonkale's temp files and the root `.gitignore`.
    struct Filter {
        root: PathBuf,
        gitignore: Gitignore,
    }

    impl Filter {
        fn new(root: &Path) -> Self {
            let mut b = GitignoreBuilder::new(root);
            let _ = b.add(root.join(".gitignore"));
            Self {
                root: root.to_path_buf(),
                gitignore: b.build().unwrap_or_else(|_| Gitignore::empty()),
            }
        }

        /// The relative path to report, or `None` to drop the event.
        fn keep(&self, path: &Path) -> Option<String> {
            let rel = path.strip_prefix(&self.root).ok()?;
            if rel.as_os_str().is_empty() {
                return None;
            }
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            if rel_str.split('/').any(|c| c.starts_with('.')) || rel_str.ends_with(".moonkale-tmp")
            {
                return None;
            }
            let is_dir = path.is_dir();
            if self
                .gitignore
                .matched_path_or_any_parents(rel, is_dir)
                .is_ignore()
            {
                return None;
            }
            Some(rel_str)
        }
    }

    fn run(
        mut watcher: RecommendedWatcher,
        mut watched: HashSet<PathBuf>,
        rx: mpsc::Receiver<notify::Result<notify::Event>>,
        root: PathBuf,
        filter: Filter,
        shared: Arc<Shared>,
        stop: Arc<AtomicBool>,
    ) {
        while !stop.load(Ordering::Relaxed) {
            let event = match rx.recv_timeout(Duration::from_millis(500)) {
                Ok(Ok(e)) => e,
                Ok(Err(_)) => continue,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            };
            if matches!(event.kind, EventKind::Access(_) | EventKind::Other) {
                continue;
            }
            let mut changed = Vec::new();
            for path in &event.paths {
                let Some(rel) = filter.keep(path) else {
                    continue;
                };
                // A directory that appeared (created, moved in): watch it
                // and everything visible below it.
                if path.is_dir() && !watched.contains(path) {
                    let _ = watch_tree(&mut watcher, &mut watched, &root, path);
                }
                // A watched directory that went away: forget it (the OS
                // dropped its watch already).
                if !path.exists() {
                    watched.retain(|w| !w.starts_with(path));
                }
                changed.push(rel);
            }
            if changed.is_empty() {
                continue;
            }
            {
                let mut log = shared.log.lock().unwrap();
                for rel in changed {
                    log.seq += 1;
                    let seq = log.seq;
                    log.entries.push_back((seq, rel));
                    if log.entries.len() > LOG_CAP {
                        if let Some((s, _)) = log.entries.pop_front() {
                            log.floor = s;
                        }
                    }
                }
            }
            shared.wake.notify_waiters();
        }
    }
}
