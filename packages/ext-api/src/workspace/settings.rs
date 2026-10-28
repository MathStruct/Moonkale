//! `Workspace`: user and workspace settings, the secret store. Split out of `workspace.rs` in Milestone 18 phase 3b
//! (no API change; phase 3c turns the areas into services).

use super::*;
use crate::{t, L};

impl Workspace {
    /// A record from the state store, if there is a store and the record.
    pub async fn state_get<R: moonkale_state::Record>(
        &self,
        key: &moonkale_state::Key,
    ) -> Option<R> {
        let s = self.config.persistence.state?;
        match (s.get)(R::TABLE.to_string(), key.as_bytes().to_vec()).await {
            Ok(Some(bytes)) => moonkale_state::decode(&bytes)
                .map_err(|e| tracing::warn!("state: {} ignored: {e}", R::TABLE))
                .ok(),
            Ok(None) => None,
            Err(e) => {
                tracing::warn!("state: reading {}: {e}", R::TABLE);
                None
            }
        }
    }

    /// Store a record (nothing without a store; failures are logged).
    pub async fn state_put<R: moonkale_state::Record>(
        &self,
        key: &moonkale_state::Key,
        record: &R,
    ) {
        let Some(s) = self.config.persistence.state else {
            return;
        };
        let batch = moonkale_state::put_in(moonkale_state::Batch::new(), key, record);
        if let Err(e) = (s.write)(batch).await {
            tracing::warn!("state: writing {}: {e}", R::TABLE);
        }
    }

    /// Whether there is a store on the open folder's host
    /// (`Persistence::host`); without one, folder state stays in files.
    pub fn has_host_state(&self) -> bool {
        self.config.persistence.host.is_some()
    }

    async fn migrate_host_sessions(&self, h: StateAccess, key: &[u8]) -> Result<(), String> {
        let mut parts = moonkale_state::Key::reader(key);
        let folder = parts.str().ok_or("missing conversation folder")?;
        let kind = parts.str().ok_or("missing conversation kind")?;
        let prefix = moonkale_state::Key::new().str(&folder).str(&kind);
        let marker = prefix.clone().str("$sessions-v1").into_bytes();
        let table = moonkale_state::tables::AGENT_SESSIONS;
        if (h.get)(table.into(), marker.clone()).await?.is_some() {
            return Ok(());
        }
        let mut batch = moonkale_state::Batch::new();
        for (k, v) in (h.scan)(table.into(), prefix.as_bytes().to_vec()).await? {
            let mut tail = moonkale_state::Key::reader(&k[prefix.as_bytes().len()..]);
            if tail.str().as_deref() == Some("$session") {
                continue;
            }
            let mut scoped = prefix
                .clone()
                .str("$session")
                .str(super::DEFAULT_SESSION)
                .into_bytes();
            scoped.extend(&k[prefix.as_bytes().len()..]);
            batch = batch.put(table, scoped, v).delete(table, k);
        }
        (h.write)(batch.put(table, marker, b"1".to_vec())).await
    }

    fn host_session_key_in(&self, table: &str, key: &[u8], session: &str) -> Vec<u8> {
        if table == moonkale_state::tables::AGENT_SESSIONS {
            let mut parts = moonkale_state::Key::reader(key);
            if let (Some(folder), Some(kind)) = (parts.str(), parts.str()) {
                let prefix = moonkale_state::Key::new().str(&folder).str(&kind);
                let mut scoped = prefix.clone().str("$session").str(session).into_bytes();
                scoped.extend(&key[prefix.as_bytes().len()..]);
                return scoped;
            }
        }
        key.to_vec()
    }

    fn host_session_key(&self, table: &str, key: &[u8]) -> Vec<u8> {
        self.host_session_key_in(table, key, &self.saved_sessions.active.peek())
    }

    /// A record from the folder host's store (`None`: absent, no store, or
    /// unreadable — logged).
    pub async fn host_get<R: moonkale_state::Record>(
        &self,
        key: &moonkale_state::Key,
    ) -> Option<R> {
        let h = self.config.persistence.host?;
        if R::TABLE == moonkale_state::tables::AGENT_SESSIONS {
            self.migrate_host_sessions(h, key.as_bytes()).await.ok()?;
        }
        match (h.get)(
            R::TABLE.to_string(),
            self.host_session_key(R::TABLE, key.as_bytes()),
        )
        .await
        {
            Ok(Some(bytes)) => moonkale_state::decode(&bytes)
                .map_err(|e| tracing::warn!("host state: {} ignored: {e}", R::TABLE))
                .ok(),
            Ok(None) => None,
            Err(e) => {
                tracing::warn!("host state: reading {}: {e}", R::TABLE);
                None
            }
        }
    }

    /// The records under `prefix` in the folder host's store, in key order;
    /// unreadable ones are skipped. `Err`: no store, or it failed.
    pub async fn host_scan<R: moonkale_state::Record>(
        &self,
        prefix: &moonkale_state::Key,
    ) -> Result<Vec<(Vec<u8>, R)>, String> {
        let h = self
            .config
            .persistence
            .host
            .ok_or_else(|| "no host store".to_string())?;
        if R::TABLE == moonkale_state::tables::AGENT_SESSIONS {
            self.migrate_host_sessions(h, prefix.as_bytes()).await?;
        }
        let rows = (h.scan)(
            R::TABLE.to_string(),
            self.host_session_key(R::TABLE, prefix.as_bytes()),
        )
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(|(k, v)| match moonkale_state::decode(&v) {
                Ok(r) => Some((k, r)),
                Err(e) => {
                    tracing::warn!("host state: {} ignored: {e}", R::TABLE);
                    None
                }
            })
            .collect())
    }

    /// Apply a batch to the folder host's store. `Err`: no store, or it failed.
    pub async fn host_write(&self, batch: moonkale_state::Batch) -> Result<(), String> {
        let session = self.saved_sessions.active.peek().clone();
        self.host_write_in_session(&session, batch).await
    }

    /// Persist a background conversation to its captured owning session.
    pub async fn host_write_in_session(
        &self,
        session: &str,
        batch: moonkale_state::Batch,
    ) -> Result<(), String> {
        let h = self
            .config
            .persistence
            .host
            .ok_or_else(|| "no host store".to_string())?;
        let mut batch = batch;
        for op in &batch.ops {
            match op {
                moonkale_state::Op::Put { table, key, .. }
                | moonkale_state::Op::Delete { table, key }
                    if table == moonkale_state::tables::AGENT_SESSIONS =>
                {
                    self.migrate_host_sessions(h, key).await?
                }
                _ => {}
            }
        }
        for op in &mut batch.ops {
            match op {
                moonkale_state::Op::Put { table, key, .. }
                | moonkale_state::Op::Delete { table, key } => {
                    *key = self.host_session_key_in(table, key, session);
                }
            }
        }
        (h.write)(batch).await
    }

    /// Whether the folder's settings file on disk still has a layout part
    /// (a file from before the state store), which the next save removes.
    async fn folder_file_has_layout(&self, folder: &SourceId) -> bool {
        let Some(node) = self
            .node_at_path(folder, crate::settings::WORKSPACE_FILE)
            .await
        else {
            return false;
        };
        let Some(src) = self.source(folder) else {
            return false;
        };
        match src.fetch_text(node.id).await {
            Ok((text, _)) => crate::settings::SettingsFile::parse(&text)
                .map(|f| !crate::settings::LayoutRecord::of(&f).is_empty())
                .unwrap_or(false),
            Err(_) => false,
        }
    }

    /// Whether the user's settings are persisted on this platform.
    pub fn has_settings_store(&self) -> bool {
        self.config.persistence.settings.is_some() || self.user_settings_state().is_some()
    }

    /// The platform's secret store, if any.
    pub fn secret_store(&self) -> Option<SecretStore> {
        self.config.persistence.secrets
    }

    // ---- settings -------------------------------------------------------

    /// Recompute the resolved settings from the two scopes + environment.
    pub(super) fn resolve_settings(&mut self) {
        let resolved = crate::settings::Settings::resolve(
            &{
                let mut user = self.settings.user.peek().clone();
                user.overlay(&self.saved_sessions.settings.peek());
                user
            },
            &self.settings.workspace.peek(),
            &crate::settings::Settings::env_overrides(),
        );
        if *self.settings.resolved.peek() != resolved {
            self.settings.resolved.set(resolved);
        }
    }

    /// The state store, when it holds the user scope.
    fn user_settings_state(&self) -> Option<StateAccess> {
        let p = &self.config.persistence;
        p.state.filter(|_| p.user_settings_in_state)
    }

    /// The user scope: from the state store (importing what the platform's
    /// settings store has, once), else from the platform's store. `None`:
    /// neither exists.
    async fn read_user_settings(&self) -> Option<Result<crate::settings::SettingsFile, String>> {
        use crate::settings::UserSettingsRecord;
        let legacy = self.config.persistence.settings;
        if let Some(state) = self.user_settings_state() {
            let key = UserSettingsRecord::key().into_bytes();
            match (state.get)(moonkale_state::tables::SETTINGS.into(), key).await {
                Ok(Some(bytes)) => {
                    return Some(
                        moonkale_state::decode::<UserSettingsRecord>(&bytes)
                            .map(|r| r.0)
                            .map_err(|e| e.to_string()),
                    )
                }
                Ok(None) => {
                    let file = match legacy {
                        Some(s) => (s.load)().await,
                        None => Ok(crate::settings::SettingsFile::new()),
                    };
                    if let Ok(f) = &file {
                        self.state_put(&UserSettingsRecord::key(), &UserSettingsRecord(f.clone()))
                            .await;
                        tracing::info!("settings: the user settings moved into the state store");
                    }
                    return Some(file);
                }
                Err(e) => {
                    tracing::warn!("settings: the state store failed ({e}); the platform's file")
                }
            }
        }
        match legacy {
            Some(s) => Some((s.load)().await),
            None => None,
        }
    }

    /// Load the user scope (at startup).
    pub async fn load_user_settings(mut self) {
        // Runs on the client after the first render: now the system's
        // language may show (spec 030).
        let system = crate::i18n::system_language();
        if *self.settings.system_language.peek() != system {
            self.settings.system_language.set(system);
        }
        let Some(loaded) = self.read_user_settings().await else {
            return;
        };
        match loaded {
            Ok(file) => {
                self.settings.user.set(file);
                self.resolve_settings();
            }
            Err(e) => self.set_status(t!(self, L, "settings-not-loaded", error = e)),
        }
        self.refresh_wasm_extensions().await;
        if let Err(e) = self.load_sessions().await {
            self.set_status(e);
        }
        // Desktop: a remote folder asked for on the command line wins
        // (Milestone 11) …
        if let Some((host, path)) = self.config.network.remote.and_then(|r| (r.at_start)()) {
            tracing::info!("remote: opening {host}:{path} at start");
            self.open_remote(host, path);
            return;
        }
        // … else come back to where you were.
        if self.config.folders.reopen_last
            && self.sources.open.peek().is_empty()
            && self.settings.user.peek().reopen_last != Some(false)
        {
            let last = self
                .settings
                .resolved
                .peek()
                .recent_folders
                .first()
                .cloned();
            if let Some(path) = last {
                tracing::info!("settings: reopening last folder {path}");
                if let Err(e) = self.open_folder(path).await {
                    self.set_status(t!(self, L, "reopen-failed", error = e.to_string()));
                }
            }
        }
    }

    /// Change one scope and persist it (extension settings, Milestone 13).
    pub fn update_settings_in(
        self,
        target: crate::SettingsTarget,
        f: impl FnOnce(&mut crate::settings::SettingsFile) + 'static,
    ) {
        spawn(async move {
            match target {
                crate::SettingsTarget::Session => self.update_session_settings(f).await,
                crate::SettingsTarget::User => self.update_user_settings(f).await,
                crate::SettingsTarget::Workspace => self.update_workspace_settings(f).await,
            }
        });
    }

    /// Change this session's overrides and write them to the state store.
    pub async fn update_session_settings(
        mut self,
        f: impl FnOnce(&mut crate::settings::SettingsFile),
    ) {
        let mut file = self.saved_sessions.settings.peek().clone();
        f(&mut file);
        self.saved_sessions.settings.set(file);
        self.resolve_settings();
        if let Err(e) = self.persist_session().await {
            self.set_status(e);
        }
    }

    /// Change the user scope and persist it.
    pub async fn update_user_settings(
        mut self,
        f: impl FnOnce(&mut crate::settings::SettingsFile),
    ) {
        let mut file = self.settings.user.peek().clone();
        f(&mut file);
        self.settings.user.set(file.clone());
        self.resolve_settings();
        let saved = if let Some(state) = self.user_settings_state() {
            let record = crate::settings::UserSettingsRecord(file);
            let batch = moonkale_state::put_in(
                moonkale_state::Batch::new(),
                &crate::settings::UserSettingsRecord::key(),
                &record,
            );
            (state.write)(batch).await
        } else if let Some(store) = self.config.persistence.settings {
            (store.save)(file).await
        } else {
            Ok(())
        };
        if let Err(e) = saved {
            self.set_status(t!(self, L, "settings-not-saved", error = e));
        }
    }

    /// Read `.moonkale/settings.json` of `folder` (missing = defaults).
    pub async fn load_workspace_settings(mut self, folder: &SourceId) {
        let file = match self
            .node_at_path(folder, crate::settings::WORKSPACE_FILE)
            .await
        {
            Some(node) => match self.source(folder) {
                Some(src) => match src.fetch_text(node.id).await {
                    Ok((text, _)) => {
                        crate::settings::SettingsFile::parse(&text).unwrap_or_else(|e| {
                            self.set_status(t!(
                                self,
                                L,
                                "workspace-settings-ignored",
                                error = e.to_string()
                            ));
                            crate::settings::SettingsFile::new()
                        })
                    }
                    Err(_) => crate::settings::SettingsFile::new(),
                },
                None => crate::settings::SettingsFile::new(),
            },
            None => crate::settings::SettingsFile::new(),
        };
        // The layout part lives in the state store (ADR-0014); a folder file
        // from before that carries it once, which moves it over.
        let mut file = file;
        if self.config.persistence.state.is_some() {
            let key = self.session_key().str(folder.as_str());
            match self.state_get::<crate::settings::LayoutRecord>(&key).await {
                Some(rec) => rec.apply_to(&mut file),
                None => {
                    let old = crate::settings::LayoutRecord::of(&file);
                    if !old.is_empty() {
                        self.state_put(&key, &old).await;
                    }
                }
            }
        }
        tracing::info!(
            "settings: workspace {} → layout {}, {} open documents, active {:?}",
            folder,
            file.layout.is_some(),
            file.open_documents.len(),
            file.active_document
        );
        self.settings.folder.set(Some(folder.clone()));
        self.settings.workspace.set(file);
        self.resolve_settings();
        self.load_history(folder).await;
        self.join_presence(folder.as_str());
    }

    /// Change the workspace scope and write it into the folder.
    pub async fn update_workspace_settings(
        mut self,
        f: impl FnOnce(&mut crate::settings::SettingsFile),
    ) {
        let before = self.settings.workspace.peek().clone();
        let mut file = before.clone();
        f(&mut file);
        self.settings.workspace.set(file.clone());
        self.resolve_settings();
        let Some(folder) = self.settings.folder.peek().clone() else {
            return;
        };
        // With a state store the layout goes there and the folder file keeps
        // only what the folder shares; a layout change no longer touches it.
        if self.config.persistence.state.is_some() {
            use crate::settings::LayoutRecord;
            let layout = LayoutRecord::of(&file);
            if layout != LayoutRecord::of(&before) {
                let key = self.session_key().str(folder.as_str());
                self.state_put(&key, &layout).await;
            }
            if LayoutRecord::strip(&file) == LayoutRecord::strip(&before)
                && !self.folder_file_has_layout(&folder).await
            {
                return;
            }
        }
        self.write_folder_settings(folder).await;
    }

    /// Write the folder's settings file from the current state. Saves never
    /// overlap: one that starts while another is writing only asks it to run
    /// once more, and each run builds the text from the state at that moment,
    /// so the file ends with the newest settings. Overlapping writes lost
    /// changes and, through the folder source's `(mtime, len)` versions,
    /// could replace only part of the file (the first start with a state store
    /// saves twice in quick succession: "version conflict" on Android,
    /// 2026-10-15).
    async fn write_folder_settings(mut self, folder: SourceId) {
        let mut gate = self.settings.folder_write;
        if gate.peek().writing {
            gate.write().again = true;
            return;
        }
        gate.set(FolderWrite {
            writing: true,
            again: false,
        });
        // Reopen the gate even if this task is dropped mid-write.
        struct Reopen(Signal<FolderWrite>);
        impl Drop for Reopen {
            fn drop(&mut self) {
                self.0.set(FolderWrite::default());
            }
        }
        let _reopen = Reopen(gate);
        loop {
            gate.write().again = false;
            self.write_folder_settings_once(&folder).await;
            if !gate.peek().again {
                break;
            }
        }
    }

    async fn write_folder_settings_once(&mut self, folder: &SourceId) {
        let Some(src) = self.source(folder) else {
            return;
        };
        let latest = self.settings.workspace.peek().clone();
        let text = if self.config.persistence.state.is_some() {
            crate::settings::LayoutRecord::strip(&latest).to_json()
        } else {
            latest.to_json()
        };
        let result = match self
            .node_at_path(folder, crate::settings::WORKSPACE_FILE)
            .await
        {
            Some(node) => match src.fetch_text(node.id).await {
                // A listing hint is never a save precondition. Refuse unreadable
                // state instead of fabricating an empty body and a version.
                Ok((on_disk, version)) => {
                    if on_disk == text {
                        return;
                    }
                    src.apply(Transaction::write_text(
                        node.id,
                        version,
                        TextPatch::whole(&text, on_disk.chars().count()),
                    ))
                    .await
                }
                Err(error) => Err(error),
            },
            None => {
                let root = src.descriptor().root;
                src.apply(Transaction::create_text(
                    root,
                    crate::settings::WORKSPACE_FILE,
                    text,
                ))
                .await
            }
        };
        match result {
            Ok(applied) if applied.first_error().is_none() => {}
            Ok(applied) => {
                let e = applied.first_error().cloned().unwrap();
                self.set_status(t!(
                    self,
                    L,
                    "workspace-settings-not-saved",
                    error = e.to_string()
                ));
            }
            Err(e) => self.set_status(t!(
                self,
                L,
                "workspace-settings-not-saved",
                error = e.to_string()
            )),
        }
    }
}

/// Whether the folder's settings file is being written, and whether another
/// save asked for one more write meanwhile.
#[derive(Clone, Copy, Default, PartialEq)]
pub(crate) struct FolderWrite {
    writing: bool,
    again: bool,
}

/// The settings files of both scopes and what they resolve to. (Milestone 18 phase 3c: the workspace's state, grouped by area.)
#[derive(Clone, Copy)]
pub struct SettingsState {
    /// The settings everyone reads: user, folder (without authority) and environment merged.
    pub resolved: Signal<crate::settings::Settings>,
    /// Persisted scopes and the resolved value (see `settings.rs`).
    pub user: Signal<crate::settings::SettingsFile>,
    /// The open folder's settings file as read.
    pub workspace: Signal<crate::settings::SettingsFile>,
    /// The folder whose `.moonkale/settings.json` is loaded, if any.
    pub folder: Signal<Option<SourceId>>,
    /// The system's language (spec 030). English until the app has started
    /// on the client ([`Workspace::load_user_settings`] sets it): the web
    /// build renders on the server first and hydration keeps that markup, so
    /// the first render must not depend on whose system it is.
    pub system_language: Signal<String>,
    /// Serialises writes of the folder's settings file.
    pub(crate) folder_write: Signal<FolderWrite>,
}

impl SettingsState {
    pub(super) fn new() -> Self {
        Self {
            resolved: Signal::new_in_scope(
                crate::settings::Settings::resolve(
                    &crate::settings::SettingsFile::new(),
                    &crate::settings::SettingsFile::new(),
                    &crate::settings::Settings::env_overrides(),
                ),
                ScopeId::ROOT,
            ),
            user: Signal::new_in_scope(crate::settings::SettingsFile::new(), ScopeId::ROOT),
            workspace: Signal::new_in_scope(crate::settings::SettingsFile::new(), ScopeId::ROOT),
            folder: Signal::new_in_scope(None, ScopeId::ROOT),
            system_language: Signal::new_in_scope(crate::i18n::FALLBACK.to_string(), ScopeId::ROOT),
            folder_write: Signal::new_in_scope(FolderWrite::default(), ScopeId::ROOT),
        }
    }
}
