//! `IndexSource` — the derived graph as a `Source`.

use crate::extract::{symbols_julia, symbols_python, symbols_rust, wikilinks, Derived};
use crate::graph::IndexGraph;
use crate::search::{Hit, SearchIndex};
use crate::walk::{self, Limits};
use moonkale_core::{
    async_trait, Applied, Capabilities, ContentRef, Edge, Node, NodeId, NodeKind, Query,
    QueryResult, Source, SourceDescriptor, SourceError, SourceFamily, SourceId, Table, Transaction,
    Value, Version,
};
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

/// The index's locks survive a panic while held (#6): a poisoned lock is
/// taken over instead of failing every later search for the session. The
/// data may miss the one update that panicked; the next refresh repairs it.
trait Lock<T> {
    fn r(&self) -> RwLockReadGuard<'_, T>;
    fn w(&self) -> RwLockWriteGuard<'_, T>;
}

impl<T> Lock<T> for RwLock<T> {
    fn r(&self) -> RwLockReadGuard<'_, T> {
        self.read().unwrap_or_else(|e| e.into_inner())
    }
    fn w(&self) -> RwLockWriteGuard<'_, T> {
        self.write().unwrap_or_else(|e| e.into_inner())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IndexStats {
    pub files: usize,
    pub links: usize,
    pub symbols: usize,
    pub truncated: bool,
}

pub struct IndexSource {
    id: SourceId,
    folder: Arc<dyn Source>,
    root: NodeId,
    graph: RwLock<IndexGraph>,
    stats: RwLock<IndexStats>,
    limits: Limits,
    #[cfg(feature = "sophia")]
    folder_store: Option<Arc<crate::folder_store::FolderStore>>,
    #[cfg(feature = "sophia")]
    folder_store_lock: tokio::sync::Mutex<()>,
    #[cfg(feature = "sophia")]
    cache_repair_attempted: std::sync::atomic::AtomicBool,
    #[cfg(feature = "sophia")]
    folder_report: RwLock<Result<crate::folder_store::FolderReport, String>>,
    cache_reader: RwLock<Option<Arc<dyn Source>>>,
    cache_error: RwLock<Option<String>>,
    search: RwLock<SearchIndex>,
    /// Embedding provider for hybrid search (`None` = BM25 only).
    embedder: Option<Arc<dyn moonkale_llm::Provider>>,
    #[cfg(feature = "content-ids")]
    content_cache: Arc<std::sync::Mutex<crate::content::Cache>>,
    #[cfg(feature = "content-ids")]
    content_generation: std::sync::atomic::AtomicU64,
}

/// Batch size for embedding requests.
const EMBED_BATCH: usize = 32;

impl IndexSource {
    /// Build the index of `folder` now. The folder's own root node becomes
    /// the index root, so the index's `Children(root)` is the folder's tree
    /// enriched with derived nodes.
    pub async fn build(folder: Arc<dyn Source>) -> Result<Self, SourceError> {
        Self::build_with(folder, None).await
    }

    /// Like [`build`](Self::build), with an embedding provider: chunks get
    /// vectors (call [`embed_pending`](Self::embed_pending) to fill them)
    /// and `search` fuses BM25 with cosine similarity.
    pub async fn build_with(
        folder: Arc<dyn Source>,
        embedder: Option<Arc<dyn moonkale_llm::Provider>>,
    ) -> Result<Self, SourceError> {
        let fd = folder.descriptor();
        let id = SourceId::new(format!("index:{}", fd.id));
        #[cfg(not(feature = "sophia"))]
        let cache_error = Some("Sophia unavailable in this build".to_string());
        #[cfg(feature = "sophia")]
        let cache_error;
        #[cfg(feature = "sophia")]
        let folder_store = if fd.truth == moonkale_core::SourceTruth::Store {
            cache_error = Some("store truth: code graph writes await SPEC-010".into());
            None
        } else if let Some(path) = fd.id.as_str().strip_prefix("folder:") {
            let path = std::path::PathBuf::from(path);
            match tokio::task::spawn_blocking(move || crate::folder_store::FolderStore::open(path))
                .await
            {
                Ok(Ok(store)) => {
                    cache_error = None;
                    Some(Arc::new(store))
                }
                Ok(Err(error)) => {
                    cache_error = Some(cache_failure("cache open failed", &error));
                    None
                }
                Err(_) => {
                    cache_error = Some("cache open worker failed".into());
                    None
                }
            }
        } else {
            cache_error = Some("source has no local cache directory".into());
            None
        };
        let this = Self {
            id,
            root: fd.root,
            folder,
            graph: RwLock::new(IndexGraph::default()),
            stats: RwLock::new(IndexStats::default()),
            limits: Limits::default(),
            #[cfg(feature = "sophia")]
            folder_store,
            #[cfg(feature = "sophia")]
            folder_store_lock: tokio::sync::Mutex::new(()),
            #[cfg(feature = "sophia")]
            cache_repair_attempted: std::sync::atomic::AtomicBool::new(false),
            #[cfg(feature = "sophia")]
            folder_report: RwLock::new(match &cache_error {
                Some(reason) => Err(reason.clone()),
                None => Ok(Default::default()),
            }),
            cache_reader: RwLock::new(None),
            cache_error: RwLock::new(cache_error),
            search: RwLock::new(SearchIndex::default()),
            embedder: embedder.filter(|e| e.supports_embed()),
            #[cfg(feature = "content-ids")]
            content_cache: Arc::new(std::sync::Mutex::new(crate::content::Cache::default())),
            #[cfg(feature = "content-ids")]
            content_generation: std::sync::atomic::AtomicU64::new(0),
        };
        this.rebuild().await?;
        #[cfg(feature = "sophia")]
        let _ = this.sync_folder_store().await;
        Ok(this)
    }

    /// Attach the read-only adapter without exposing any Sophia types.
    pub fn set_cache_reader(&self, reader: Result<Arc<dyn Source>, SourceError>) {
        match reader {
            Ok(reader) => *self.cache_reader.w() = Some(reader),
            Err(error) => {
                *self.cache_error.w() = Some(cache_failure("cache reader open failed", &error))
            }
        }
    }

    /// The optional code store is an additional Source, opened by the app.
    #[cfg(feature = "sophia")]
    pub fn folder_store_path(&self) -> Option<std::path::PathBuf> {
        if self.folder_report.r().is_ok() {
            self.folder_store.as_ref().map(|s| s.path())
        } else {
            None
        }
    }

    #[cfg(feature = "sophia")]
    pub fn folder_store_report(&self) -> Result<crate::folder_store::FolderReport, String> {
        self.folder_report.r().clone()
    }

    #[cfg(feature = "sophia")]
    async fn sync_folder_store(&self) -> Result<(), SourceError> {
        let Some(store) = &self.folder_store else {
            return Ok(());
        };
        let _serial = self.folder_store_lock.lock().await;
        let result = async {
            let files = self.cache_files(store).await?;
            let store = store.clone();
            let may_repair = !self
                .cache_repair_attempted
                .load(std::sync::atomic::Ordering::Relaxed);
            let (result, repaired) = tokio::task::spawn_blocking(move || {
                let result = store.sync(&files);
                if result.is_err() && may_repair && store.is_broken() {
                    (store.rebuild(&files), true)
                } else {
                    (result, false)
                }
            })
            .await
            .map_err(|e| SourceError::Io(e.to_string()))?;
            if repaired {
                self.cache_repair_attempted
                    .store(true, std::sync::atomic::Ordering::Relaxed);
            }
            result
        }
        .await;
        *self.cache_error.w() = result
            .as_ref()
            .err()
            .map(|error| cache_failure("cache sync failed", error));
        *self.folder_report.w() = result.clone().map_err(|e| e.to_string());
        result.map(|_| ())
    }

    #[cfg(feature = "sophia")]
    async fn cache_files(
        &self,
        store: &crate::folder_store::FolderStore,
    ) -> Result<Vec<crate::folder_store::CodeFile>, SourceError> {
        let (entries, truncated) = walk::walk(&*self.folder, self.root, &self.limits).await?;
        if truncated {
            return Err(SourceError::Invalid("folder-store walk truncated".into()));
        }
        let mut files = Vec::new();
        let mut bytes = 0usize;
        for (_, node) in entries {
            if node.kind != NodeKind::File
                || !walk::wants_text(&node, &self.limits)
                || !matches!(&node.content, Some(ContentRef::Text { lang: Some(l), .. }) if matches!(l.as_str(), "rust" | "julia" | "python"))
            {
                continue;
            }
            if !store.admits_path(&node.native_key) {
                continue;
            }
            let (text, _) = match self.folder.fetch_text(node.id).await {
                Ok(text) => text,
                // The index has already exposed index_error and removed
                // symbols for a non-UTF-8 file. Remove its cached facts too.
                Err(SourceError::Unsupported(_)) => continue,
                Err(error) => return Err(error),
            };
            bytes = bytes.saturating_add(text.len());
            if text.len() > self.limits.max_text_bytes as usize {
                continue;
            }
            if bytes > crate::folder_store::MAX_SNAPSHOT_BYTES {
                return Err(SourceError::Invalid("folder-store byte limit".into()));
            }
            files.push(crate::folder_store::CodeFile { node, text });
        }
        Ok(files)
    }

    #[cfg(feature = "sophia")]
    async fn check_cache(&self) -> Result<QueryResult, SourceError> {
        if self.folder.descriptor().truth == moonkale_core::SourceTruth::Store {
            return Err(SourceError::Unsupported(
                "store truth: code graph writes await SPEC-010".into(),
            ));
        }
        let store = self
            .folder_store
            .as_ref()
            .ok_or_else(|| SourceError::Unsupported("cache unavailable".into()))?;
        let _serial = self.folder_store_lock.lock().await;
        let files = self.cache_files(store).await?;
        let store = store.clone();
        let diff = tokio::task::spawn_blocking(move || store.check(&files))
            .await
            .map_err(|e| SourceError::Io(e.to_string()))??;
        *self.cache_error.w() = None;
        Ok(message_result(if diff.is_empty() {
            "same".into()
        } else {
            format!("cache problem; rebuilt: {}", diff.join(", "))
        }))
    }

    /// Attach an optional rebuildable persistent content-id cache. No source
    /// text is persisted. Ordinary constructors retain bounded memory caching.
    #[cfg(feature = "content-ids")]
    pub fn with_content_id_store(self, store: Arc<dyn moonkale_state::StateStore>) -> Self {
        self.content_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .set_store(store);
        self
    }

    /// Compute one definition and its reachable same-file dependencies on demand.
    /// Work and store I/O run on a blocking worker, never the input/UI thread.
    /// Refreshing the index during a request rejects the result as stale.
    #[cfg(feature = "content-ids")]
    pub async fn content_id(
        &self,
        definition: NodeId,
    ) -> Result<crate::content::ContentIdentity, crate::content::ContentIdError> {
        use crate::content::{ContentIdError as Error, MAX_FILE_BYTES};
        use std::sync::atomic::Ordering;
        let (file, selector, indexed_input, generation) = {
            let graph = self.graph.r();
            let node = graph
                .node(definition)
                .filter(|n| n.kind.is_definition())
                .ok_or(Error::NotDefinition)?;
            let file_id = graph
                .edges()
                .find(|e| e.kind == moonkale_core::EdgeKind::Defines && e.to == definition)
                .map(|e| e.from)
                .ok_or(Error::NotDefinition)?;
            let file = graph.node(file_id).cloned().ok_or(Error::NotDefinition)?;
            (
                file,
                node.native_key.clone(),
                node.props.get("input").cloned(),
                self.content_generation.load(Ordering::SeqCst),
            )
        };
        if matches!(
            &file.content,
            Some(moonkale_core::ContentRef::Text { lang: Some(l), .. }) if l == "julia"
        ) {
            return self.julia_content_id(definition, file, generation).await;
        }
        let (text, _) = self
            .folder
            .fetch_text(file.id)
            .await
            .map_err(|_| Error::SourceUnavailable)?;
        if text.len() > MAX_FILE_BYTES {
            return Err(Error::LimitExceeded);
        }
        let cache = self.content_cache.clone();
        let index = self.id.clone();
        let worker_file = file.clone();
        let (identities, input) = tokio::task::spawn_blocking(move || {
            if crate::extract::provenance(&text).get("input") != indexed_input.as_ref() {
                return Err(Error::StaleInput);
            }
            let input = *blake3::hash(text.as_bytes()).as_bytes();
            // File selector scopes location-keyed cache payloads; never part of the content digest.
            let key = crate::content::Cache::key(&input, &format!("{}:{}", index, selector));
            let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
            let identities =
                if let Some(cached) = cache.get(&key).filter(|c| c.contains_key(&definition)) {
                    cached
                } else {
                    let calculated =
                        crate::content::calculate(&index, &worker_file, &text, definition)?;
                    cache.put(key, calculated.clone());
                    calculated
                };
            Ok::<_, Error>((identities, input))
        })
        .await
        .map_err(|_| Error::WorkerFailed)??;
        // Fetch again before publication: an external edit may precede watcher refresh.
        let (fresh, _) = self
            .folder
            .fetch_text(file.id)
            .await
            .map_err(|_| Error::StaleInput)?;
        if fresh.len() > MAX_FILE_BYTES {
            return Err(Error::LimitExceeded);
        }
        let unchanged = tokio::task::spawn_blocking(move || {
            blake3::hash(fresh.as_bytes()).as_bytes() == &input
        })
        .await
        .map_err(|_| Error::WorkerFailed)?;
        if !unchanged {
            return Err(Error::StaleInput);
        }
        self.publish_content_ids(definition, identities, generation)
    }

    /// Publishes a request's identities on their definition nodes, unless
    /// the index was refreshed meanwhile; returns the requested one.
    #[cfg(feature = "content-ids")]
    fn publish_content_ids(
        &self,
        definition: NodeId,
        identities: std::collections::HashMap<NodeId, crate::content::ContentIdentity>,
        generation: u64,
    ) -> Result<crate::content::ContentIdentity, crate::content::ContentIdError> {
        use crate::content::ContentIdError as Error;
        use std::sync::atomic::Ordering;
        let mut graph = self.graph.w();
        if self.content_generation.load(Ordering::SeqCst) != generation
            || graph.node(definition).is_none()
        {
            return Err(Error::StaleInput);
        }
        if identities.keys().any(|id| !graph.nodes.contains_key(id)) {
            return Err(Error::StaleInput);
        }
        for (id, identity) in &identities {
            let node = graph.nodes.get_mut(id).ok_or(Error::StaleInput)?;
            node.props
                .insert("content_id".into(), Value::Text(identity.id.to_string()));
            node.props.insert(
                "content_digest".into(),
                Value::Text(
                    blake3::Hash::from_bytes(identity.digest)
                        .to_hex()
                        .to_string(),
                ),
            );
            node.props.insert(
                "content_normalizer".into(),
                Value::Int(identity.normalizer_version as i64),
            );
            node.props.insert(
                "content_component".into(),
                Value::Text(
                    blake3::Hash::from_bytes(identity.component_digest)
                        .to_hex()
                        .to_string(),
                ),
            );
            node.props.insert(
                "content_component_index".into(),
                Value::Int(identity.component_index as i64),
            );
            node.props.insert(
                "content_component_size".into(),
                Value::Int(identity.component_size as i64),
            );
        }
        identities
            .get(&definition)
            .cloned()
            .ok_or(Error::NotDefinition)
    }

    /// Julia (normalizer v5): the definition's module is its package (the
    /// nearest `Project.toml` naming `src/<name>.jl`) when the package's
    /// include graph reaches the file, otherwise the file itself. Every file
    /// of the module is read, checked against the index and read again
    /// before publication.
    #[cfg(feature = "content-ids")]
    async fn julia_content_id(
        &self,
        definition: NodeId,
        file: Node,
        generation: u64,
    ) -> Result<crate::content::ContentIdentity, crate::content::ContentIdError> {
        use crate::content::{ContentIdError as Error, JuliaFile};
        let mut module = match self.julia_package_root(&file).await {
            Some(root) => self.julia_module_files(root).await?,
            None => (Vec::new(), Vec::new()),
        };
        if !module.0.iter().any(|f| f.node.id == file.id) {
            module = self.julia_module_files(file.clone()).await?;
        }
        let (files, dynamic) = module;
        // The index's definitions must come from these bytes.
        for f in &files {
            let indexed = {
                let graph = self.graph.r();
                let input = graph
                    .edges()
                    .find(|e| e.kind == moonkale_core::EdgeKind::Defines && e.from == f.node.id)
                    .and_then(|e| graph.node(e.to))
                    .and_then(|n| n.props.get("input").cloned());
                input
            };
            if indexed.is_some_and(|i| crate::extract::provenance(&f.text).get("input") != Some(&i))
            {
                return Err(Error::StaleInput);
            }
        }
        let cache = self.content_cache.clone();
        let index = self.id.clone();
        let (identities, files) = tokio::task::spawn_blocking(move || {
            // The whole module is the input: every file's path, module
            // path and bytes, in include order, and the dynamic includes.
            let mut input = blake3::Hasher::new();
            for f in &files {
                for part in [
                    f.node.native_key.as_bytes(),
                    f.module.join(".").as_bytes(),
                    f.text.as_bytes(),
                ] {
                    input.update(&(part.len() as u64).to_le_bytes());
                    input.update(part);
                }
            }
            for d in &dynamic {
                input.update(b"dynamic:");
                input.update(d.join(".").as_bytes());
            }
            let input = *input.finalize().as_bytes();
            let key = crate::content::Cache::key(&input, &format!("{}:julia-module", index));
            let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
            let identities =
                if let Some(cached) = cache.get(&key).filter(|c| c.contains_key(&definition)) {
                    cached
                } else {
                    let calculated =
                        crate::content::calculate_julia(&index, &files, &dynamic, definition)?;
                    cache.put(key, calculated.clone());
                    calculated
                };
            Ok::<_, Error>((identities, files))
        })
        .await
        .map_err(|_| Error::WorkerFailed)??;
        // Read every file again: an external edit may precede the watcher.
        for JuliaFile { node, text, .. } in &files {
            let (fresh, _) = self
                .folder
                .fetch_text(node.id)
                .await
                .map_err(|_| Error::StaleInput)?;
            if &fresh != text {
                return Err(Error::StaleInput);
            }
        }
        self.publish_content_ids(definition, identities, generation)
    }

    /// The root file of the Julia package a file is in: the nearest
    /// `Project.toml` with `name = "X"` whose `src/X.jl` exists.
    #[cfg(feature = "content-ids")]
    async fn julia_package_root(&self, file: &Node) -> Option<Node> {
        let mut dir = file.native_key.as_str();
        loop {
            dir = dir.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
            let join = |rel: &str| {
                if dir.is_empty() {
                    rel.to_string()
                } else {
                    format!("{dir}/{rel}")
                }
            };
            let project = self.graph.r().file_by_path(&join("Project.toml"));
            if let Some(project) = project {
                let (text, _) = self.folder.fetch_text(project).await.ok()?;
                let name = text.lines().take(64).find_map(|l| {
                    let (k, v) = l.split_once('=')?;
                    (k.trim() == "name").then(|| v.trim().trim_matches('"').to_string())
                })?;
                let graph = self.graph.r();
                return graph
                    .file_by_path(&join(&format!("src/{name}.jl")))
                    .and_then(|id| graph.node(id).cloned());
            }
            if dir.is_empty() {
                return None;
            }
        }
    }

    /// A module's files from its root, following literal includes depth
    /// first; an include that is not a literal, leaves the folder or names a
    /// missing file is dynamic.
    #[cfg(feature = "content-ids")]
    async fn julia_module_files(
        &self,
        root: Node,
    ) -> Result<(Vec<crate::content::JuliaFile>, Vec<Vec<String>>), crate::content::ContentIdError>
    {
        use crate::content::{ContentIdError as Error, JuliaFile};
        use moonkale_content_id::julia::{MAX_MODULE_BYTES, MAX_MODULE_FILES};
        let mut files: Vec<JuliaFile> = Vec::new();
        let mut dynamic = Vec::new();
        let mut bytes = 0;
        let mut pending = vec![(root, Vec::new())];
        while let Some((node, module)) = pending.pop() {
            if files.iter().any(|f| f.node.id == node.id) {
                continue;
            }
            if files.len() >= MAX_MODULE_FILES {
                return Err(Error::LimitExceeded);
            }
            let (text, _) = self
                .folder
                .fetch_text(node.id)
                .await
                .map_err(|_| Error::SourceUnavailable)?;
            bytes += text.len();
            if bytes > MAX_MODULE_BYTES {
                return Err(Error::LimitExceeded);
            }
            let (text, found) = tokio::task::spawn_blocking(move || {
                let tree = moonkale_content_id::parse(
                    "julia",
                    &text,
                    &mut moonkale_content_id::Budget::new(),
                )?;
                let found = moonkale_content_id::julia::includes(&text, &tree);
                Ok::<_, Error>((text, found))
            })
            .await
            .map_err(|_| Error::WorkerFailed)??;
            let dir = node
                .native_key
                .rsplit_once('/')
                .map(|(d, _)| d)
                .unwrap_or("");
            let mut next = Vec::new();
            for include in found {
                let mut at = module.clone();
                at.extend(include.module);
                let target = include.path.and_then(|p| relative(dir, &p)).and_then(|p| {
                    let graph = self.graph.r();
                    graph
                        .file_by_path(&p)
                        .and_then(|id| graph.node(id).cloned())
                });
                match target {
                    Some(target) => next.push((target, at)),
                    None => dynamic.push(at),
                }
            }
            // Depth first, in source order.
            pending.extend(next.into_iter().rev());
            files.push(JuliaFile { node, text, module });
        }
        Ok((files, dynamic))
    }

    /// Embed every chunk that has no vector yet, in batches. Safe to call
    /// repeatedly (after refreshes); a no-op without an embedder.
    pub async fn embed_pending(&self) -> Result<usize, String> {
        let Some(e) = &self.embedder else {
            return Ok(0);
        };
        let mut done = 0;
        loop {
            let batch = self.search.r().pending_embeddings(EMBED_BATCH);
            if batch.is_empty() {
                break;
            }
            let texts: Vec<String> = batch.iter().map(|(_, t)| t.clone()).collect();
            let vectors = e.embed(texts).await?;
            let mut s = self.search.w();
            for ((i, _), v) in batch.into_iter().zip(vectors) {
                s.set_embedding(i, v);
                done += 1;
            }
        }
        Ok(done)
    }

    /// `(chunks, embedded)` for the status line.
    pub fn search_stats(&self) -> (usize, usize) {
        let s = self.search.r();
        (s.chunk_count(), s.embedded_count())
    }

    async fn run_search(&self, query: &str, limit: usize) -> Result<Vec<Hit>, SourceError> {
        // The query embedding is one network call; when the provider is slow
        // or unreachable the keyword half must still answer (BM25 alone),
        // so it gets a short budget.
        let qv = match &self.embedder {
            Some(e) if self.search.r().embedded_count() > 0 => {
                match tokio::time::timeout(
                    std::time::Duration::from_secs(3),
                    e.embed(vec![query.to_string()]),
                )
                .await
                {
                    Ok(Ok(mut v)) => v.pop(),
                    Ok(Err(err)) => {
                        tracing::warn!("search: query embedding failed, keyword only: {err}");
                        None
                    }
                    Err(_) => {
                        tracing::warn!("search: query embedding took > 3 s, keyword only");
                        None
                    }
                }
            }
            _ => None,
        };
        Ok(self
            .search
            .read()
            .unwrap()
            .search(query, qv.as_deref(), limit))
    }

    pub fn stats(&self) -> IndexStats {
        *self.stats.r()
    }

    async fn rebuild(&self) -> Result<(), SourceError> {
        let root_node = self
            .folder
            .query(Query::Node(self.root))
            .await?
            .nodes
            .into_iter()
            .next();
        let (entries, truncated) = walk::walk(&*self.folder, self.root, &self.limits).await?;
        let mut graph = IndexGraph::default();
        if let Some(r) = root_node {
            graph.insert_node(r);
        }
        let mut contains: std::collections::HashMap<NodeId, Vec<Edge>> = Default::default();
        for (parent, node) in &entries {
            graph.insert_node(node.clone());
            contains.entry(*parent).or_default().push(Edge::contains(
                &node.source,
                *parent,
                node.id,
            ));
        }
        for (parent, edges) in contains {
            graph.set_derived(parent, Vec::new(), edges);
        }
        *self.graph.w() = graph;
        *self.search.w() = SearchIndex::default();

        let mut files = 0;
        for (_, node) in entries {
            if node.kind == NodeKind::File {
                files += 1;
                if walk::wants_text(&node, &self.limits) {
                    self.extract_file(&node).await;
                }
            }
        }
        self.graph.w().resolve_imports();
        let g = self.graph.r();
        *self.stats.w() = IndexStats {
            files,
            links: g.count_edge_kind(&moonkale_core::EdgeKind::Links),
            symbols: g.count_kind(&NodeKind::definition()),
            truncated,
        };
        Ok(())
    }

    /// Re-run extraction for the given files (after their link targets
    /// changed); ignores files that vanished meanwhile.
    async fn re_extract(&self, files: &[NodeId]) {
        for id in files {
            let node = self.graph.r().node(*id).cloned();
            let Some(node) = node else { continue };
            if node.kind != NodeKind::File || !walk::wants_text(&node, &self.limits) {
                continue;
            }
            self.extract_file(&node).await;
        }
    }

    fn update_counts(&self) {
        let g = self.graph.r();
        let mut s = self.stats.w();
        s.files = g.count_kind(&NodeKind::File);
        s.links = g.count_edge_kind(&moonkale_core::EdgeKind::Links);
        s.symbols = g.count_kind(&NodeKind::definition());
    }

    /// Keep a content-free diagnostic on the mirrored node. Never index lossy text.
    async fn extract_file(&self, file: &Node) {
        match self.folder.fetch_text(file.id).await {
            Ok((text, _)) => {
                if let Some(node) = self.graph.w().nodes.get_mut(&file.id) {
                    node.props.remove("index_error");
                }
                self.extract_into(file, &text);
            }
            Err(_) => {
                self.search.w().remove_file(file.id);
                let mut graph = self.graph.w();
                graph.set_derived(file.id, Vec::new(), Vec::new());
                if let Some(node) = graph.nodes.get_mut(&file.id) {
                    node.props
                        .insert("index_error".into(), Value::Text("text read failed".into()));
                }
            }
        }
    }

    fn extract_into(&self, file: &Node, text: &str) {
        self.search.w().set_file(file, text);
        let derived = {
            let g = self.graph.r();
            let mut d = Derived::default();
            match file.content.as_ref().and_then(|c| match c {
                ContentRef::Text { lang, .. } => lang.as_deref(),
                _ => None,
            }) {
                Some("markdown") => {
                    let mut links = wikilinks::extract(&g, &self.id, file, text);
                    let provenance = crate::extract::provenance(text);
                    for node in &mut links.nodes {
                        node.props.extend(provenance.clone());
                    }
                    for edge in &mut links.edges {
                        edge.props.extend(provenance.clone());
                    }
                    d.merge(links);
                }
                Some("rust") => d.merge(symbols_rust::extract(&self.id, file, text)),
                Some("julia") => d.merge(symbols_julia::extract(&self.id, file, text)),
                Some("python") => d.merge(symbols_python::extract(&self.id, file, text)),
                _ => {}
            }
            d
        };
        let mut graph = self.graph.w();
        #[cfg(feature = "content-ids")]
        {
            self.content_generation
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            // No previously computed identity may outlive its dependency input.
            let affected: Vec<_> = graph
                .edges()
                .filter(|e| e.from == file.id && e.kind == moonkale_core::EdgeKind::Defines)
                .map(|e| e.to)
                .collect();
            for id in affected {
                let Some(node) = graph.nodes.get_mut(&id) else {
                    continue;
                };
                for key in [
                    "content_id",
                    "content_digest",
                    "content_normalizer",
                    "content_component",
                    "content_component_index",
                    "content_component_size",
                ] {
                    node.props.remove(key);
                }
            }
        }
        graph.set_derived(file.id, derived.nodes, derived.edges);
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Source for IndexSource {
    fn id(&self) -> SourceId {
        self.id.clone()
    }

    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            truth: self.folder.descriptor().truth,
            id: self.id.clone(),
            display_name: {
                let s = self.stats();
                format!(
                    "index: {} files · {} links · {} symbols{}",
                    s.files,
                    s.links,
                    s.symbols,
                    if s.truncated { " (truncated)" } else { "" }
                )
            },
            family: SourceFamily::Index,
            capabilities: Capabilities {
                read: true,
                write: false,
                watch: false,
                text_query: None,
                extra_text_query: None,
            },
            root: self.root,
        }
    }

    async fn query(&self, query: Query) -> Result<QueryResult, SourceError> {
        if let Query::Text { dialect, text } = &query {
            if dialect == "code-graph-status" {
                let reason = self.cache_error.r().clone().or_else(|| {
                    self.cache_reader
                        .r()
                        .is_none()
                        .then(|| "cache reader unavailable".into())
                });
                return Ok(message_result(reason.unwrap_or_default()));
            }
            #[cfg(feature = "sophia")]
            if dialect == "check-code-cache" {
                return self.check_cache().await;
            }
            if dialect == "index-node" {
                if text.len() > 8192 {
                    return Err(SourceError::Invalid("selection query limit".into()));
                }
                let selected: Node = serde_json::from_str(text)
                    .map_err(|_| SourceError::Invalid("invalid selection".into()))?;
                if selected.source == self.id {
                    return self.query(Query::Node(selected.id)).await;
                }
                #[cfg(not(feature = "sophia"))]
                return Ok(QueryResult::default());
                #[cfg(feature = "sophia")]
                {
                    let reader = self.cache_reader.r().clone();
                    let Some(reader) = reader.filter(|r| r.id() == selected.source) else {
                        return Ok(QueryResult::default());
                    };
                    if !selected.kind.is_definition() {
                        return Ok(QueryResult::default());
                    }
                    {
                        let graph = self.graph.r();
                        if graph
                            .nodes
                            .values()
                            .any(|n| n.kind.is_definition() && n.native_key == selected.native_key)
                        {
                            let nodes: Vec<_> = graph.nodes.values().cloned().collect();
                            return Ok(crate::selection::index_node(&selected, &nodes, &[], &[])
                                .cloned()
                                .map(QueryResult::single)
                                .unwrap_or_default());
                        }
                    }
                    let locations = reader
                        .query(Query::Text {
                            dialect: "definition-locations".into(),
                            text: selected.native_key.clone(),
                        })
                        .await?;
                    let graph = self.graph.r();
                    let nodes: Vec<_> = graph.nodes.values().cloned().collect();
                    let edges: Vec<_> = graph.edges().cloned().collect();
                    return Ok(crate::selection::index_node(
                        &selected,
                        &nodes,
                        &edges,
                        locations
                            .table
                            .as_ref()
                            .map(|t| t.rows.as_slice())
                            .unwrap_or_default(),
                    )
                    .cloned()
                    .map(QueryResult::single)
                    .unwrap_or_default());
                }
            }
            if dialect == "code-graph" {
                // The envelope is a bounded host protocol, not arbitrary nested queries.
                if text.len() > 8192 {
                    return Err(SourceError::Invalid("graph query limit".into()));
                }
                let (memory, query): (bool, Query) = serde_json::from_str(text)
                    .map_err(|_| SourceError::Invalid("invalid graph query".into()))?;
                if !matches!(
                    query,
                    Query::All { .. }
                        | Query::Neighbours { .. }
                        | Query::Coarse { .. }
                        | Query::Node(_)
                ) {
                    return Err(SourceError::Unsupported("graph query".into()));
                }
                let reader = if !memory && self.cache_error.r().is_none() {
                    self.cache_reader.r().clone()
                } else {
                    None
                };
                if let Some(reader) = reader {
                    let cache_query = match &query {
                        Query::All { limit, .. } => Query::All {
                            limit: *limit,
                            kinds: None,
                        },
                        Query::Neighbours {
                            node,
                            depth,
                            direction,
                        } => {
                            let file = {
                                let graph = self.graph.r();
                                graph
                                    .node(*node)
                                    .filter(|n| n.kind == NodeKind::File)
                                    .cloned()
                                    .or_else(|| {
                                        graph
                                            .edges()
                                            .find(|e| {
                                                e.kind == moonkale_core::EdgeKind::Defines
                                                    && e.to == *node
                                            })
                                            .and_then(|e| graph.node(e.from))
                                            .cloned()
                                    })
                            };
                            match file {
                                Some(file)
                                    if matches!(
                                        file.language_hint(),
                                        Some("rust" | "julia" | "python")
                                    ) =>
                                {
                                    Query::Text {
                                        dialect: "folder-neighbours".into(),
                                        text: serde_json::to_string(&(
                                            file.native_key,
                                            depth,
                                            direction,
                                        ))
                                        .map_err(|e| SourceError::Invalid(e.to_string()))?,
                                    }
                                }
                                // Notes and other knowledge nodes remain index-owned.
                                Some(_) => return self.query(query).await,
                                None => query.clone(),
                            }
                        }
                        _ => query.clone(),
                    };
                    match reader.query(cache_query).await {
                        Ok(mut result) => {
                            if matches!(
                                query,
                                Query::All { .. } | Query::Coarse { region: None, .. }
                            ) {
                                // Keep the file/knowledge graph while code definitions,
                                // names and code relations come exclusively from Sophia.
                                let limit = match query {
                                    Query::All { limit, .. } => {
                                        limit.saturating_sub(result.nodes.len())
                                    }
                                    _ => 10_000,
                                };
                                let mut files = self
                                    .query(Query::All {
                                        limit,
                                        kinds: Some(vec![
                                            NodeKind::File,
                                            NodeKind::Directory,
                                            NodeKind::Page,
                                        ]),
                                    })
                                    .await?;
                                let ids: std::collections::HashSet<_> =
                                    files.nodes.iter().map(|n| n.id).collect();
                                files
                                    .edges
                                    .retain(|e| ids.contains(&e.from) && ids.contains(&e.to));
                                result.nodes.extend(files.nodes);
                                result.edges.extend(files.edges);
                                result.truncated |= files.truncated;
                            }
                            return Ok(result);
                        }
                        Err(SourceError::NotFound)
                            if matches!(query, Query::Neighbours { .. } | Query::Node(_)) =>
                        {
                            return Ok(QueryResult::default())
                        }
                        Err(error) => {
                            *self.cache_error.w() = Some(cache_failure("cache read failed", &error))
                        }
                    }
                }
                return self.query(query).await;
            }
        }
        if let Query::Coarse { level, region } = query {
            return moonkale_core::source::coarse::query(self, level, region).await;
        }
        let g = self.graph.r();
        match query {
            Query::Coarse { .. } => unreachable!("dispatched before locking"),
            Query::Node(id) => {
                let node = g.node(id).cloned().ok_or(SourceError::NotFound)?;
                #[cfg(feature = "sophia")]
                let node = {
                    let mut node = node;
                    if id == self.root {
                        match self.folder_store_report() {
                            Ok(r) => {
                                node.props.insert(
                                    "sophia_refused".into(),
                                    Value::Int(r.refused_definitions as i64),
                                );
                                node.props.insert(
                                    "sophia_naming_diagnostics".into(),
                                    Value::Text(r.naming_diagnostics.join("\n")),
                                );
                            }
                            Err(error) => {
                                node.props.insert("sophia_error".into(), Value::Text(error));
                            }
                        }
                    }
                    node
                };
                Ok(QueryResult::single(node))
            }
            Query::Children(id) => {
                let ids = g.children(id);
                let nodes: Vec<Node> = ids.iter().filter_map(|i| g.node(*i).cloned()).collect();
                let edges = g
                    .edges()
                    .filter(|e| e.from == id && ids.contains(&e.to))
                    .cloned()
                    .collect();
                Ok(QueryResult {
                    nodes,
                    edges,
                    ..Default::default()
                })
            }
            Query::Neighbours {
                node,
                depth,
                direction,
            } => {
                let (ids, edges) = g.neighbours(node, depth, direction);
                let nodes = ids.iter().filter_map(|i| g.node(*i).cloned()).collect();
                Ok(QueryResult {
                    nodes,
                    edges,
                    ..Default::default()
                })
            }
            Query::All { limit, kinds } => {
                let (nodes, edges, truncated) = g.all(limit, kinds.as_deref());
                Ok(QueryResult {
                    nodes,
                    edges,
                    table: None,
                    truncated,
                })
            }
            Query::Text { dialect, text } if dialect == "search" => {
                drop(g);
                let hits = self.run_search(text.trim(), 20).await?;
                let g = self.graph.r();
                let mut nodes: Vec<Node> = Vec::new();
                for h in &hits {
                    if !nodes.iter().any(|n| n.id == h.file) {
                        if let Some(n) = g.node(h.file) {
                            nodes.push(n.clone());
                        }
                    }
                }
                Ok(QueryResult {
                    nodes,
                    table: Some(Table {
                        columns: vec![
                            "path".into(),
                            "line".into(),
                            "score".into(),
                            "snippet".into(),
                        ],
                        rows: hits
                            .iter()
                            .map(|h| {
                                vec![
                                    Value::Ref {
                                        node: h.file,
                                        label: h.path.clone(),
                                    },
                                    Value::Int(h.line as i64),
                                    Value::Float(((h.score as f64) * 1000.0).round() / 1000.0),
                                    Value::Text(h.snippet.clone()),
                                ]
                            })
                            .collect(),
                        truncated: false,
                    }),
                    ..Default::default()
                })
            }
            Query::Text { dialect, .. } => Err(SourceError::Unsupported(format!(
                "dialect {dialect}; the index speaks `search`"
            ))),
        }
    }

    async fn fetch_text(&self, node: NodeId) -> Result<(String, Version), SourceError> {
        // Mirrored file nodes read through the folder; derived nodes have no body.
        let owner = self
            .graph
            .read()
            .unwrap()
            .node(node)
            .map(|n| n.source.clone())
            .ok_or(SourceError::NotFound)?;
        if owner == self.folder.id() {
            self.folder.fetch_text(node).await
        } else {
            Err(SourceError::Unsupported(
                "derived nodes have no text".into(),
            ))
        }
    }

    async fn apply(&self, _tx: Transaction) -> Result<Applied, SourceError> {
        Err(SourceError::Unsupported(
            "the index is read-only; write through the folder".into(),
        ))
    }

    async fn refresh(&self, node: NodeId) -> Result<(), SourceError> {
        self.refresh_batch(&[node]).await
    }

    async fn refresh_batch(&self, nodes: &[NodeId]) -> Result<(), SourceError> {
        for node in nodes {
            self.refresh_one(*node).await?;
        }
        self.graph.w().resolve_imports();
        #[cfg(feature = "sophia")]
        let _ = self.sync_folder_store().await;
        Ok(())
    }
}

impl IndexSource {
    /// Refresh the in-memory graph; the outer batch publishes Sophia once.
    async fn refresh_one(&self, node: NodeId) -> Result<(), SourceError> {
        let found = match self.folder.query(Query::Node(node)).await {
            Ok(r) => r.nodes.into_iter().next(),
            Err(SourceError::NotFound) => None,
            Err(e) => return Err(e),
        };
        let Some(top) = found else {
            let (gone, linkers) = {
                let mut g = self.graph.w();
                // Files that linked here: their links become unresolved
                // (collect before the edges are dropped with the subtree).
                let linkers = g.origins_linking_to(&g.subtree(node));
                let gone = g.remove_subtree(node);
                (gone, linkers)
            };
            {
                let mut search = self.search.w();
                for g in &gone {
                    search.remove_file(*g);
                }
            }
            self.re_extract(&linkers).await;
            self.update_counts();
            return Ok(());
        };
        let mut entries: Vec<(NodeId, Node)> = Vec::new();
        {
            let parent_rel = top
                .native_key
                .rsplit_once('/')
                .map(|(p, _)| p)
                .unwrap_or("");
            let parent = NodeId::derive(&top.source, parent_rel);
            entries.push((parent, top.clone()));
        }
        if top.kind == NodeKind::Directory {
            let (below, _) = walk::walk(self.folder.as_ref(), node, &self.limits).await?;
            entries.extend(below);
        }
        {
            let mut g = self.graph.w();
            for (parent, n) in &entries {
                g.insert_node(n.clone());
                g.add_edge(*parent, Edge::contains(&n.source, *parent, n.id));
            }
        }
        for (_, n) in &entries {
            if n.kind == NodeKind::File && walk::wants_text(n, &self.limits) {
                self.extract_file(n).await;
            }
        }
        // Links that were unresolved and now have a target.
        let linkers = {
            let g = self.graph.r();
            let phantoms: Vec<NodeId> = entries
                .iter()
                .filter(|(_, n)| n.kind == NodeKind::File)
                .flat_map(|(_, n)| g.phantoms_for(&n.native_key))
                .collect();
            g.origins_linking_to(&phantoms)
        };
        self.re_extract(&linkers).await;
        self.update_counts();
        Ok(())
    }
}

/// `rel` joined to the folder-relative directory `dir`, normalised; `None`
/// when it is absolute or leaves the folder.
#[cfg(feature = "content-ids")]
fn relative(dir: &str, rel: &str) -> Option<String> {
    if rel.starts_with('/') || rel.contains('\\') {
        return None;
    }
    let mut parts: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
    for part in rel.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            p => parts.push(p),
        }
    }
    Some(parts.join("/"))
}

fn message_result(message: String) -> QueryResult {
    QueryResult {
        table: Some(Table {
            columns: vec!["message".into()],
            rows: vec![vec![Value::Text(message)]],
            truncated: false,
        }),
        ..Default::default()
    }
}

fn cache_failure(action: &str, error: &SourceError) -> String {
    // Structural read/storage diagnostics only; bound status/remote payloads.
    format!(
        "{action}: {}",
        error.to_string().chars().take(240).collect::<String>()
    )
}

#[cfg(test)]
mod lock_tests {
    use super::Lock;
    use std::sync::{Arc, RwLock};

    #[test]
    fn a_poisoned_lock_is_taken_over() {
        let lock = Arc::new(RwLock::new(1));
        let l = lock.clone();
        let _ = std::thread::spawn(move || {
            let _g = l.write().unwrap();
            panic!("poison it");
        })
        .join();
        assert!(lock.is_poisoned());
        *lock.w() += 1;
        assert_eq!(*lock.r(), 2);
    }
}
