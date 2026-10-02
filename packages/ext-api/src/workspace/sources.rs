//! `Workspace`: open, attach, follow, close and query sources; files by path. Split out of `workspace.rs` in Milestone 18 phase 3b
//! (no API change; phase 3c turns the areas into services).

use super::*;

impl Workspace {
    /// Open a source another window already has (no-op if we have it).
    /// Close a source (spec 015): its documents go (unsaved ones block the
    /// close with a status message), derived sources over it (the index)
    /// go with it, and its workspace state is left on disk. Closing the
    /// folder the workspace settings belong to also drops the history log
    /// and the presence room; nothing is reopened on the next start.
    pub async fn close_source(mut self, id: &SourceId) -> Result<(), SourceError> {
        let Some(handle) = self
            .sources
            .peek()
            .iter()
            .find(|s| &s.descriptor.id == id)
            .cloned()
        else {
            return Err(SourceError::NotFound);
        };
        // Everything derived from this source closes too.
        let derived: Vec<SourceId> = self
            .sources
            .peek()
            .iter()
            .filter(|s| {
                s.descriptor
                    .id
                    .as_str()
                    .ends_with(&format!(":{}", id.as_str()))
                    && s.descriptor.family == moonkale_core::SourceFamily::Index
            })
            .map(|s| s.descriptor.id.clone())
            .collect();
        let closing: Vec<SourceId> = std::iter::once(id.clone()).chain(derived).collect();
        let docs: Vec<(NodeId, bool)> = self
            .documents
            .peek()
            .iter()
            .filter(|(_, d)| closing.contains(&d.peek().node.source))
            .map(|(n, d)| (*n, d.peek().dirty()))
            .collect();
        let dirty = docs.iter().filter(|(_, d)| *d).count();
        if dirty > 0 {
            self.set_status(format!(
                "{}: save or reload {dirty} unsaved document(s) before closing it",
                handle.descriptor.display_name
            ));
            return Err(SourceError::Invalid(format!("{dirty} unsaved document(s)")));
        }
        for (n, _) in docs {
            self.close_node(n);
        }
        if self.settings_folder.peek().as_ref() == Some(id) {
            self.persist_history().await;
            self.history.set(moonkale_core::EntityLog::new());
            self.settings_workspace
                .set(crate::settings::SettingsFile::new());
            self.settings_folder.set(None);
            self.presence_link.set(None);
            self.presence.set(Vec::new());
            self.resolve_settings();
        }
        self.sources
            .with_mut(|v| v.retain(|s| !closing.contains(&s.descriptor.id)));
        self.graph_epoch.with_mut(|e| *e += 1);
        if self.is_remote_source(id) {
            self.close_remote();
        } else {
            self.update_user_settings(|f| f.reopen_last = Some(false))
                .await;
        }
        self.set_status(format!("Closed {}", handle.descriptor.display_name));
        Ok(())
    }

    /// Can this platform open folders over SSH?
    /// The source openers of this build ([`WorkspaceConfig::openers`]).
    pub fn openers(&self) -> &'static moonkale_core::Openers {
        self.config.folders.openers
    }

    pub async fn attach_source(mut self, descriptor: SourceDescriptor) -> Result<(), SourceError> {
        if self.source(&descriptor.id).is_some() {
            return Ok(());
        }
        let source = (self.config.folders.attach)(descriptor.clone()).await?;
        self.sources
            .with_mut(|v| v.push(SourceHandle { descriptor, source }));
        Ok(())
    }

    /// The raw bytes of a node (images, spec 008).
    pub async fn fetch_bytes(&self, node: &Node) -> Result<Vec<u8>, SourceError> {
        let source = self.source(&node.source).ok_or(SourceError::NotFound)?;
        source.fetch_bytes(node.id).await.map(|(b, _)| b)
    }

    /// The text of `rel` under `source`, `None` if it does not exist or is
    /// not readable (a small config file such as `.moonkale/katex.json`).
    pub async fn read_text_at(&self, source: &SourceId, rel: &str) -> Option<String> {
        let node = self.node_at_path(source, rel).await?;
        let src = self.source(source)?;
        src.fetch_text(node.id).await.ok().map(|(text, _)| text)
    }

    /// Write `rel` under `source`, creating it (and its parents, where the
    /// source does) or replacing the whole text. Small state files only
    /// (`.moonkale/agent-sessions/local/<id>.json`).
    pub async fn write_text_at(
        self,
        source: &SourceId,
        rel: &str,
        text: &str,
    ) -> Result<(), SourceError> {
        let src = self.source(source).ok_or(SourceError::NotFound)?;
        let result = match self.node_at_path(source, rel).await {
            Some(node) => {
                let chars = src
                    .fetch_text(node.id)
                    .await
                    .map(|(t, _)| t.chars().count())
                    .unwrap_or(0);
                src.apply(Transaction::write_text(
                    node.id,
                    node.version,
                    TextPatch::whole(text, chars),
                ))
                .await?
            }
            None => {
                let root = src.descriptor().root;
                src.apply(Transaction::create_text(root, rel, text)).await?
            }
        };
        match result.first_error() {
            Some(e) => Err(e.clone()),
            None => Ok(()),
        }
    }

    /// The entries of the directory `rel` under `source`, hidden ones
    /// included (the folder source's `ls` dialect; empty for other sources
    /// or a missing directory).
    pub async fn list_at(&self, source: &SourceId, rel: &str) -> Vec<Node> {
        let Some(src) = self.source(source) else {
            return Vec::new();
        };
        src.query(Query::Text {
            dialect: "ls".into(),
            text: rel.to_string(),
        })
        .await
        .map(|r| r.nodes)
        .unwrap_or_default()
    }

    /// Resolve a relative path in a source: the folder source's `path`
    /// dialect (hidden files too), else a walk of the tree.
    pub async fn node_at_path(&self, source: &SourceId, rel: &str) -> Option<Node> {
        let src = self.source(source)?;
        if let Ok(r) = src
            .query(Query::Text {
                dialect: "path".into(),
                text: rel.into(),
            })
            .await
        {
            return r.nodes.into_iter().next();
        }
        let mut cur = src.descriptor().root;
        let mut found = None;
        for part in rel.split('/').filter(|p| !p.is_empty()) {
            let res = src.query(Query::Children(cur)).await.ok()?;
            let n = res.nodes.into_iter().find(|n| n.label == part)?;
            cur = n.id;
            found = Some(n);
        }
        found
    }

    /// Absolute path of a folder node, when its source is a local folder.
    pub fn folder_path(&self, node: &Node) -> Option<String> {
        let root = node.source.as_str().strip_prefix("folder:")?;
        Some(if node.native_key.is_empty() {
            root.to_string()
        } else {
            format!("{root}/{}", node.native_key)
        })
    }

    /// Whether this platform has a native folder dialog.
    pub fn has_folder_dialog(&self) -> bool {
        self.config.folders.pick.is_some()
    }

    /// The first open folder's path (what `git` and terminals run in).
    pub fn folder_root(&self) -> Option<String> {
        self.sources
            .peek()
            .iter()
            .find(|s| s.descriptor.family == moonkale_core::SourceFamily::Folder)
            .and_then(|s| {
                s.descriptor
                    .id
                    .as_str()
                    .strip_prefix("folder:")
                    .map(str::to_string)
            })
    }

    /// Show the native folder dialog (if any) and open the chosen folder.
    /// `Ok(None)` means cancelled or no dialog on this platform.
    pub async fn open_folder_dialog(mut self) -> Result<Option<SourceDescriptor>, SourceError> {
        let Some(pick) = self.config.folders.pick else {
            self.set_status("No folder dialog on this platform — type a path in Sources");
            return Ok(None);
        };
        match pick().await {
            Some(path) => self.open_folder(path).await.map(Some),
            None => Ok(None),
        }
    }

    pub fn source(&self, id: &SourceId) -> Option<Arc<dyn Source>> {
        self.sources
            .read()
            .iter()
            .find(|s| &s.descriptor.id == id)
            .map(|s| s.source.clone())
    }

    /// Add an in-process source (a parsed trace, a scratch graph) to this
    /// window. Not announced to the session bus: it has no path to reopen.
    pub fn add_source(&mut self, source: Arc<dyn Source>) -> SourceDescriptor {
        let descriptor = source.descriptor();
        self.sources.with_mut(|v| {
            v.retain(|s| s.descriptor.id != descriptor.id);
            v.push(SourceHandle {
                descriptor: descriptor.clone(),
                source,
            });
        });
        descriptor
    }

    /// Open a folder through the platform's factory and add it to `sources`.
    pub async fn open_folder(mut self, path: String) -> Result<SourceDescriptor, SourceError> {
        let options = {
            let s = self.settings.peek();
            OpenOptions {
                embed: (s.search.embeddings && s.llm.embed_model.is_some()
                    || s.search.embeddings && s.llm.provider == "mock")
                    .then(|| s.llm.clone()),
            }
        };
        tracing::info!("open_folder: {path}");
        let sources = (self.config.folders.open)(path, options).await?;
        tracing::info!("open_folder: {} sources", sources.len());
        let mut first: Option<SourceDescriptor> = None;
        let mut opened: Vec<SourceId> = Vec::new();
        for source in sources {
            let descriptor = source.descriptor();
            opened.push(descriptor.id.clone());
            self.sources.with_mut(|v| {
                v.retain(|s| s.descriptor.id != descriptor.id);
                v.push(SourceHandle {
                    descriptor: descriptor.clone(),
                    source,
                });
            });
            self.send(SessionMessage::SourceOpened {
                from: self.window.peek().clone(),
                descriptor: descriptor.clone(),
            });
            first.get_or_insert(descriptor);
        }
        let first = first.ok_or_else(|| SourceError::Invalid("nothing opened".into()))?;
        self.set_status(format!("Opened {}", self.sources_summary()));
        tracing::info!("open_folder: first {} ({:?})", first.id, first.family);
        let remote = self
            .remote
            .peek()
            .as_ref()
            .is_some_and(|r| r.phase == crate::remote::RemotePhase::Ready);
        if remote {
            // Opened through the SSH session: remember it there, not in the
            // recent folders (the path is not on this machine).
            self.remote.with_mut(|r| {
                if let Some(r) = r {
                    r.sources.extend(opened.iter().cloned());
                }
            });
        }
        let via_server = self.server_link.peek().is_some();
        if via_server {
            self.server_link.with_mut(|l| {
                if let Some((_, ids)) = l {
                    ids.extend(opened.iter().cloned());
                }
            });
        }
        if first.family == moonkale_core::SourceFamily::Folder && !remote && !via_server {
            // Workspace settings + remember the folder.
            self.load_workspace_settings(&first.id).await;
            self.refresh_wasm_extensions().await;
            let path = first
                .id
                .as_str()
                .strip_prefix("folder:")
                .unwrap_or(first.id.as_str())
                .to_string();
            self.update_user_settings(|f| {
                f.push_recent(&path);
                f.reopen_last = Some(true);
            })
            .await;
        }
        Ok(first)
    }

    /// "folder · index: 12 files · 30 links" — for the status bar.
    pub fn sources_summary(&self) -> String {
        self.sources
            .peek()
            .iter()
            .map(|s| s.descriptor.display_name.clone())
            .collect::<Vec<_>>()
            .join(" · ")
    }

    /// The index source, if one is open (derived data: links, symbols).
    pub fn index(&self) -> Option<SourceHandle> {
        self.sources
            .peek()
            .iter()
            .find(|s| s.descriptor.family == moonkale_core::SourceFamily::Index)
            .cloned()
    }

    pub async fn query(&self, source: &SourceId, query: Query) -> Result<QueryResult, SourceError> {
        let s = self.source(source).ok_or(SourceError::NotFound)?;
        s.query(query).await
    }

    /// Tell the other sources (the index) about a node that changed or
    /// vanished, then bump the epochs the panels watch.
    pub(super) async fn after_fs_change(&mut self, source_id: &SourceId, nodes: &[NodeId]) {
        let others: Vec<Arc<dyn Source>> = self
            .sources
            .peek()
            .iter()
            .filter(|s| &s.descriptor.id != source_id)
            .map(|s| s.source.clone())
            .collect();
        for other in others {
            for n in nodes {
                let _ = other.refresh(*n).await;
            }
        }
        self.graph_epoch.with_mut(|e| *e += 1);
        self.fs_epoch.with_mut(|e| *e += 1);
    }

    /// Follow every open source that is not followed yet (Milestone 16): one
    /// long-poll loop per source on `Source::changes_since`. Cheap to call
    /// often — the shell calls it whenever `sources` changes.
    pub fn follow_sources(self) {
        let ids: Vec<SourceId> = self
            .sources
            .peek()
            .iter()
            .map(|s| s.descriptor.id.clone())
            .collect();
        let mut followed = self.followed;
        for id in ids {
            if followed.peek().contains(&id) {
                continue;
            }
            followed.with_mut(|f| {
                f.insert(id.clone());
            });
            // Root-owned: the loop outlives whichever component asked.
            dioxus::core::spawn_forever(self.follow(id));
        }
    }

    pub(super) async fn follow(mut self, id: SourceId) {
        let mut since = 0u64;
        let mut failures = 0u32;
        loop {
            // Closed while the last poll was pending: stop.
            let Some(source) = self.source(&id) else {
                break;
            };
            match source.changes_since(since).await {
                Ok(None) => break,
                Ok(Some(changes)) => {
                    failures = 0;
                    if !self.watched.peek().contains(&id) {
                        self.watched.with_mut(|w| {
                            w.insert(id.clone());
                        });
                    }
                    let first = since == 0;
                    since = changes.seq;
                    if first || self.source(&id).is_none() {
                        continue;
                    }
                    if changes.reset {
                        let root = source.descriptor().root;
                        self.after_fs_change(&id, &[root]).await;
                    } else if !changes.paths.is_empty() {
                        self.apply_external(&id, &source, &changes.paths).await;
                    }
                }
                Err(e) => {
                    // A server restart, a dropped connection: try again,
                    // slower each time, and show the source as not followed
                    // meanwhile. The next answer's position will not be in
                    // the new log, so it comes back as a reset.
                    failures += 1;
                    if failures == 1 {
                        tracing::info!("follow {id}: {e}");
                    }
                    if self.watched.peek().contains(&id) {
                        self.watched.with_mut(|w| {
                            w.remove(&id);
                        });
                    }
                    let secs = 2u64.saturating_pow(failures.min(5)).min(30);
                    futures_timer::Delay::new(std::time::Duration::from_secs(secs)).await;
                }
            }
        }
        self.watched.with_mut(|w| {
            w.remove(&id);
        });
        self.followed.with_mut(|f| {
            f.remove(&id);
        });
    }

    /// Paths that changed on disk: resolve each (it may be new, changed or
    /// gone), let the index re-read them, and bump the epochs the Explorer,
    /// the graph and the Changes panel follow.
    pub(super) async fn apply_external(
        &mut self,
        id: &SourceId,
        source: &Arc<dyn Source>,
        paths: &[String],
    ) {
        let mut nodes = Vec::with_capacity(paths.len());
        for path in paths {
            let query = Query::Text {
                dialect: "path".into(),
                text: path.clone(),
            };
            match source.query(query).await {
                Ok(r) => nodes.extend(r.nodes.first().map(|n| n.id)),
                // Gone: its id is derived from the path, which is what the
                // index keys it by.
                Err(SourceError::NotFound) => nodes.push(NodeId::derive(id, path)),
                Err(_) => {}
            }
        }
        tracing::info!("follow {id}: {} changed on disk", paths.len());
        self.after_fs_change(id, &nodes).await;
    }

    /// Re-read a source by hand (the ↻ button of a source that is not
    /// watched, Milestone 16): the index from the root, then the Explorer
    /// and the graph.
    pub async fn refresh_source(mut self, id: &SourceId) {
        let Some(source) = self.source(id) else {
            return;
        };
        let root = source.descriptor().root;
        self.after_fs_change(id, &[root]).await;
        self.set_status(format!("Refreshed {}", source.descriptor().display_name));
    }
}
