//! `Workspace`: user and workspace settings, the secret store. Split out of `workspace.rs` in Milestone 18 phase 3b
//! (no API change; phase 3c turns the areas into services).

use super::*;

impl Workspace {
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
            &self.settings_user.peek(),
            &self.settings_workspace.peek(),
            &crate::settings::Settings::env_overrides(),
        );
        if *self.settings.peek() != resolved {
            self.settings.set(resolved);
        }
    }

    /// Load the user scope through the platform store (at startup).
    pub async fn load_user_settings(mut self) {
        let Some(store) = self.config.persistence.settings else {
            return;
        };
        match (store.load)().await {
            Ok(file) => {
                self.settings_user.set(file);
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
            && self.sources.peek().is_empty()
            && self.settings_user.peek().reopen_last != Some(false)
        {
            let last = self.settings.peek().recent_folders.first().cloned();
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
        let mut file = self.settings_user.peek().clone();
        f(&mut file);
        self.settings_user.set(file.clone());
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
        tracing::info!(
            "settings: workspace {} → layout {}, {} open documents, active {:?}",
            folder,
            file.layout.is_some(),
            file.open_documents.len(),
            file.active_document
        );
        self.settings_folder.set(Some(folder.clone()));
        self.settings_workspace.set(file);
        self.resolve_settings();
        self.load_history(folder).await;
        self.join_presence(folder.as_str());
    }

    /// Change the workspace scope and write it into the folder.
    pub async fn update_workspace_settings(
        mut self,
        f: impl FnOnce(&mut crate::settings::SettingsFile),
    ) {
        let mut file = self.settings_workspace.peek().clone();
        f(&mut file);
        self.settings_workspace.set(file.clone());
        self.resolve_settings();
        let Some(folder) = self.settings_folder.peek().clone() else {
            return;
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
