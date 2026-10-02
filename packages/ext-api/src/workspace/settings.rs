//! `Workspace`: user and workspace settings, the secret store. Split out of `workspace.rs` in Milestone 18 phase 3b
//! (no API change; phase 3c turns the areas into services).

use super::*;

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

    pub fn has_settings_store(&self) -> bool {
        self.config.persistence.settings.is_some()
    }

    pub fn secret_store(&self) -> Option<SecretStore> {
        self.config.persistence.secrets
    }

    // ---- settings -------------------------------------------------------

    /// Recompute the resolved settings from the two scopes + environment.
    pub(super) fn resolve_settings(&mut self) {
        let resolved = crate::settings::Settings::resolve(
            &self.settings.user.peek(),
            &self.settings.workspace.peek(),
            &crate::settings::Settings::env_overrides(),
        );
        if *self.settings.resolved.peek() != resolved {
            self.settings.resolved.set(resolved);
        }
    }

    /// Load the user scope through the platform store (at startup).
    pub async fn load_user_settings(mut self) {
        let Some(store) = self.config.persistence.settings else {
            return;
        };
        match (store.load)().await {
            Ok(file) => {
                self.settings.user.set(file);
                self.resolve_settings();
            }
            Err(e) => self.set_status(format!("Settings not loaded: {e}")),
        }
        self.refresh_wasm_extensions().await;
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
                    self.set_status(format!("Could not reopen the last folder: {e}"));
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
                crate::SettingsTarget::User => self.update_user_settings(f).await,
                crate::SettingsTarget::Workspace => self.update_workspace_settings(f).await,
            }
        });
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
        if let Some(store) = self.config.persistence.settings {
            if let Err(e) = (store.save)(file).await {
                self.set_status(format!("Settings not saved: {e}"));
            }
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
                            self.set_status(format!("Workspace settings ignored: {e}"));
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
            let key = moonkale_state::Key::new().str(folder.as_str());
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
        let file = if self.config.persistence.state.is_some() {
            use crate::settings::LayoutRecord;
            let layout = LayoutRecord::of(&file);
            if layout != LayoutRecord::of(&before) {
                let key = moonkale_state::Key::new().str(folder.as_str());
                self.state_put(&key, &layout).await;
            }
            let shared = LayoutRecord::strip(&file);
            if shared == LayoutRecord::strip(&before) && !self.folder_file_has_layout(&folder).await
            {
                return;
            }
            shared
        } else {
            file
        };
        let Some(src) = self.source(&folder) else {
            return;
        };
        let text = file.to_json();
        let result = match self
            .node_at_path(&folder, crate::settings::WORKSPACE_FILE)
            .await
        {
            Some(node) => {
                let chars = src
                    .fetch_text(node.id)
                    .await
                    .map(|(t, _)| t.chars().count())
                    .unwrap_or(0);
                src.apply(Transaction::write_text(
                    node.id,
                    node.version,
                    TextPatch::whole(&text, chars),
                ))
                .await
            }
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
                self.set_status(format!("Workspace settings not saved: {e}"));
            }
            Err(e) => self.set_status(format!("Workspace settings not saved: {e}")),
        }
    }
}

/// The settings files of both scopes and what they resolve to. (Milestone 18 phase 3c: the workspace's state, grouped by area.)
#[derive(Clone, Copy)]
pub struct SettingsState {
    pub resolved: Signal<crate::settings::Settings>,
    /// Persisted scopes and the resolved value (see `settings.rs`).
    pub user: Signal<crate::settings::SettingsFile>,
    pub workspace: Signal<crate::settings::SettingsFile>,
    /// The folder whose `.moonkale/settings.json` is loaded, if any.
    pub folder: Signal<Option<SourceId>>,
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
        }
    }
}
