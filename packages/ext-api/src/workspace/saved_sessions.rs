//! Named outer workspaces (ADR-0025). Live documents retain their signals.
use super::*;
use crate::settings::{LayoutRecord, SettingsFile};
use moonkale_state::{Batch, Key, Record};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Stable id used by the one-time migration.
pub const DEFAULT_SESSION: &str = "default";
const MAX_BUNDLE: usize = moonkale_state::session_bundle::MAX_BYTES;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
/// Persisted metadata of one named workspace; buffer bodies live in recovery.
pub struct SavedWorkspace {
    /// Stable opaque session identity.
    pub id: String,
    /// User-visible name.
    pub name: String,
    /// Last use in Unix milliseconds.
    pub touched: u64,
    /// Members, including unavailable sources.
    pub sources: Vec<SourceDescriptor>,
    /// Open source/path pairs.
    pub documents: Vec<(SourceId, String)>,
    /// Active source/path pair.
    pub active: Option<(SourceId, String)>,
    /// Session scope settings; secret names only.
    pub settings: SettingsFile,
    /// Saved workbench layout.
    pub layout: LayoutRecord,
    #[serde(default)]
    /// Soft-deleted session retained in the store.
    pub archived: bool,
    /// Linked panels, pins and independent subjects.
    #[serde(default)]
    pub link_groups: Vec<crate::links::LinkGroup>,
    /// Camera and detached graph instance state.
    #[serde(default)]
    pub link_views: BTreeMap<String, crate::links::LinkView>,
    /// Content instance identities, including detached views.
    #[serde(default)]
    pub link_contents: BTreeMap<String, NodeId>,
}
impl Record for SavedWorkspace {
    const TABLE: &'static str = moonkale_state::tables::SESSIONS;
    const VERSION: u32 = 2;
}
impl SavedWorkspace {
    /// Validate bounded link metadata before attaching imported or stored state.
    pub fn validate_links(&self) -> Result<(), String> {
        use crate::links::{Follow, Role};
        let id_ok = |id: &str| {
            !id.is_empty()
                && id.len() <= 128
                && id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_:".contains(&b))
        };
        let subject_ok = |s: &crate::links::Subject| {
            s.selection.len() <= 4096 && s.selection.iter().all(|id| id.len() <= 256)
        };
        let mut members = std::collections::BTreeSet::new();
        let mut ids = std::collections::BTreeSet::new();
        if self.link_groups.len() > 256
            || self.link_views.len() > 256
            || self.link_contents.len() > 4096
        {
            return Err("link metadata limits exceeded".into());
        }
        for g in &self.link_groups {
            if !id_ok(&g.id)
                || !ids.insert(&g.id)
                || !(1..=8).contains(&g.colour)
                || g.number == 0
                || g.members.is_empty()
                || g.members.len() > 64
                || g.history.is_empty()
                || g.history.len() > 256
                || g.cursor >= g.history.len()
                || !subject_ok(&g.subject)
                || g.history.iter().any(|s| !subject_ok(s))
                || g.members
                    .iter()
                    .filter(|(_, r, _)| *r == Role::Lead)
                    .count()
                    > 1
            {
                return Err("invalid link group".into());
            }
            for (id, _, follow) in &g.members {
                if !id_ok(id)
                    || !members.insert(id)
                    || matches!(follow, Follow::Pinned(s) if !subject_ok(s))
                {
                    return Err("invalid link member".into());
                }
            }
        }
        for (id, view) in &self.link_views {
            if !id_ok(id)
                || !id.starts_with("graph:")
                || !subject_ok(&view.detached)
                || view
                    .camera
                    .is_some_and(|c| c.iter().any(|n| !n.is_finite()) || c[0] <= 0.0 || c[6] <= 0.0)
            {
                return Err("invalid link view".into());
            }
        }
        if self.link_contents.keys().any(|id| !id_ok(id)) {
            return Err("invalid content view".into());
        }
        Ok(())
    }
    fn empty(id: String, name: String) -> Self {
        Self {
            id,
            name,
            touched: now_ms(),
            sources: Vec::new(),
            documents: Vec::new(),
            active: None,
            settings: SettingsFile::new(),
            layout: LayoutRecord::default(),
            archived: false,
            link_groups: Vec::new(),
            link_views: BTreeMap::new(),
            link_contents: BTreeMap::new(),
        }
    }
}

struct LiveWorkspace {
    link_groups: Vec<crate::links::LinkGroup>,
    link_views: BTreeMap<String, crate::links::LinkView>,
    link_contents: BTreeMap<String, NodeId>,
    link_results: BTreeMap<NodeId, Rc<moonkale_core::QueryResult>>,
    link_reveals: BTreeMap<String, moonkale_llm_types::display::Reveal>,
    agent_documents: std::collections::BTreeSet<NodeId>,
    sources: Vec<SourceHandle>,
    docs: Vec<(NodeId, Signal<Document>)>,
    views: Vec<Node>,
    editor_choice: std::collections::HashMap<NodeId, &'static str>,
    editor_sessions: std::collections::HashMap<NodeId, Signal<crate::editor::RevisionedDocument>>,
    editor_views: super::documents::EditorViews,
    active: Option<NodeId>,
    folder: Option<SourceId>,
    workspace: SettingsFile,
    closed: std::collections::BTreeSet<String>,
    unavailable: std::collections::BTreeSet<SourceId>,
}

#[derive(Clone, Copy)]
/// Reactive named-session state, independent of the window transport.
pub struct SavedSessions {
    /// Selected outer-session identity.
    pub active: Signal<String>,
    /// Known sessions, including archived records.
    pub list: Signal<Vec<SavedWorkspace>>,
    /// Initial migration and listing have completed.
    pub ready: Signal<bool>,
    /// Blocks periodic writers during a switch.
    pub switching: Signal<bool>,
    /// Bumped after a completed switch.
    pub generation: Signal<u64>,
    /// Session scope settings; secret names only.
    pub settings: Signal<SettingsFile>,
    suspended: Signal<bool>,
    unavailable: Signal<std::collections::BTreeSet<SourceId>>,
    live: Signal<BTreeMap<String, LiveWorkspace>>,
    gate: Signal<Rc<futures_util::lock::Mutex<()>>>,
}
impl SavedSessions {
    pub(super) fn new() -> Self {
        Self {
            active: Signal::new_in_scope(DEFAULT_SESSION.into(), ScopeId::ROOT),
            list: Signal::new_in_scope(Vec::new(), ScopeId::ROOT),
            ready: Signal::new_in_scope(false, ScopeId::ROOT),
            switching: Signal::new_in_scope(false, ScopeId::ROOT),
            generation: Signal::new_in_scope(0, ScopeId::ROOT),
            settings: Signal::new_in_scope(SettingsFile::new(), ScopeId::ROOT),
            suspended: Signal::new_in_scope(false, ScopeId::ROOT),
            unavailable: Signal::new_in_scope(Default::default(), ScopeId::ROOT),
            live: Signal::new_in_scope(BTreeMap::new(), ScopeId::ROOT),
            gate: Signal::new_in_scope(Rc::new(futures_util::lock::Mutex::new(())), ScopeId::ROOT),
        }
    }
}

impl Workspace {
    /// Prefix for state owned by the selected outer session.
    pub fn session_key(&self) -> Key {
        Key::new().str(&self.saved_sessions.active.peek())
    }

    pub(super) fn activate_session(&mut self) {
        self.saved_sessions.suspended.set(false);
    }

    /// The current session name for window chrome.
    pub fn session_name(&self) -> String {
        let id = self.saved_sessions.active.read();
        self.saved_sessions
            .list
            .read()
            .iter()
            .find(|s| s.id == *id)
            .map(|s| s.name.clone())
            .unwrap_or_else(|| "Default".into())
    }

    /// Move legacy client rows and the migration marker in one transaction.
    pub async fn load_sessions(mut self) -> Result<(), String> {
        let Some(store) = self.config.persistence.state else {
            self.saved_sessions.ready.set(true);
            return Ok(());
        };
        let marker = Key::new().str("sessions-v1");
        if (store.get)("migrations".into(), marker.as_bytes().to_vec())
            .await?
            .is_none()
        {
            let mut batch = Batch::new();
            for table in [
                moonkale_state::tables::LAYOUT,
                moonkale_state::tables::RECOVERY,
            ] {
                for (key, value) in (store.scan)(table.into(), Vec::new()).await? {
                    // A concurrent window may already have migrated these rows.
                    let mut parts = Key::reader(&key);
                    let fields = if table == moonkale_state::tables::LAYOUT {
                        1
                    } else {
                        3
                    };
                    if !(0..fields).all(|_| parts.str().is_some()) || !parts.is_empty() {
                        continue;
                    }
                    let mut scoped = Key::new().str(DEFAULT_SESSION).into_bytes();
                    scoped.extend(&key);
                    batch = batch.put(table, scoped, value).delete(table, key);
                }
            }
            let default_key = Key::new().str(DEFAULT_SESSION);
            if (store.get)(
                SavedWorkspace::TABLE.into(),
                default_key.as_bytes().to_vec(),
            )
            .await?
            .is_none()
            {
                batch = moonkale_state::put_in(
                    batch,
                    &default_key,
                    &SavedWorkspace::empty(DEFAULT_SESSION.into(), "Default".into()),
                );
            }
            batch = batch.put("migrations", marker.into_bytes(), b"1".to_vec());
            (store.write)(batch).await?;
        }
        let rows = (store.scan)(SavedWorkspace::TABLE.into(), Vec::new()).await?;
        let mut sessions = Vec::new();
        for (_, value) in rows {
            let saved =
                moonkale_state::decode::<SavedWorkspace>(&value).map_err(|e| e.to_string())?;
            saved.validate_links()?;
            sessions.push(saved);
        }
        self.saved_sessions.list.set(sessions);
        // Restore the last selected session. A new Default still follows the
        // platform's legacy reopen policy in load_user_settings.
        if let Some(bytes) = (store.get)(
            "session_state".into(),
            Key::new().str("active").into_bytes(),
        )
        .await?
        {
            let id = String::from_utf8(bytes).map_err(|_| "invalid active session")?;
            let saved = {
                self.saved_sessions
                    .list
                    .peek()
                    .iter()
                    .find(|s| s.id == id && !s.archived)
                    .cloned()
            };
            if let Some(saved) = saved {
                // Default keeps the platform's existing startup policy. Web
                // clients start blank; the saved working set remains available
                // for explicit switching and is not overwritten by that blank.
                if id == DEFAULT_SESSION && !self.config.folders.reopen_last {
                    self.saved_sessions.suspended.set(true);
                    self.saved_sessions.settings.set(saved.settings);
                    self.resolve_settings();
                    self.saved_sessions.ready.set(true);
                    return Ok(());
                }
                self.saved_sessions.switching.set(true);
                self.saved_sessions.active.set(id);
                Box::pin(self.restore_session(saved)).await;
                self.saved_sessions.switching.set(false);
                self.saved_sessions.generation.with_mut(|n| *n += 1);
            }
        }
        self.saved_sessions.ready.set(true);
        Ok(())
    }

    fn capture_session(&self) -> SavedWorkspace {
        let id = self.saved_sessions.active.peek().clone();
        let mut saved = self
            .saved_sessions
            .list
            .peek()
            .iter()
            .find(|s| s.id == id)
            .cloned()
            .unwrap_or_else(|| SavedWorkspace::empty(id, "Default".into()));
        if *self.saved_sessions.suspended.peek() {
            return saved;
        }
        saved.touched = now_ms();
        let previous_sources = saved.sources.clone();
        let previous_documents = saved.documents.clone();
        saved.sources = self
            .sources
            .open
            .peek()
            .iter()
            .filter(|s| s.descriptor.family != moonkale_core::SourceFamily::Index)
            .map(|s| s.descriptor.clone())
            .collect();
        for source in previous_sources {
            if !saved.sources.iter().any(|s| s.id == source.id)
                && self.saved_sessions.unavailable.peek().contains(&source.id)
            {
                saved.sources.push(source);
            }
        }
        saved.documents = self
            .docs
            .open
            .peek()
            .iter()
            .map(|(_, d)| {
                let d = d.peek();
                (d.node.source.clone(), d.node.native_key.clone())
            })
            .collect();
        saved.documents.extend(
            self.docs
                .views
                .peek()
                .iter()
                .filter(|node| !node.source.as_str().starts_with("agent:"))
                .map(|node| (node.source.clone(), node.native_key.clone())),
        );
        for document in previous_documents {
            if saved.sources.iter().any(|s| s.id == document.0)
                && !self
                    .sources
                    .open
                    .peek()
                    .iter()
                    .any(|s| s.descriptor.id == document.0)
                && !saved.documents.contains(&document)
            {
                saved.documents.push(document);
            }
        }
        saved.active = self
            .docs
            .active
            .peek()
            .and_then(|n| self.document(n))
            .map(|d| {
                let d = d.peek();
                (d.node.source.clone(), d.node.native_key.clone())
            });
        if saved.active.is_none() {
            saved.active = self.docs.active.peek().and_then(|id| {
                self.docs
                    .views
                    .peek()
                    .iter()
                    .find(|node| node.id == id)
                    .map(|node| (node.source.clone(), node.native_key.clone()))
            });
        }
        saved.link_groups = self
            .links
            .groups
            .peek()
            .iter()
            .filter(|g| g.agent.is_none())
            .cloned()
            .collect();
        saved.link_views = self
            .links
            .views
            .peek()
            .iter()
            .filter(|(id, _)| !id.starts_with("graph:agent:"))
            .map(|(id, v)| (id.clone(), v.clone()))
            .collect();
        saved.link_contents = self
            .links
            .contents
            .peek()
            .iter()
            .filter(|(id, _)| !id.starts_with("lead:agent:"))
            .map(|(id, v)| (id.clone(), *v))
            .collect();
        saved.settings = self.saved_sessions.settings.peek().clone();
        saved.layout = LayoutRecord::of(&self.settings.workspace.peek());
        saved
    }

    fn remember_live(&mut self) {
        if *self.saved_sessions.suspended.peek() {
            return;
        }
        let live = LiveWorkspace {
            link_groups: self.links.groups.peek().clone(),
            link_views: self.links.views.peek().clone(),
            link_contents: self.links.contents.peek().clone(),
            link_results: self.links.results.peek().clone(),
            link_reveals: self.links.reveals.peek().clone(),
            agent_documents: self.links.agent_documents.peek().clone(),
            sources: self.sources.open.peek().clone(),
            docs: self.docs.open.peek().clone(),
            views: self.docs.views.peek().clone(),
            editor_choice: self.docs.editor_choice.peek().clone(),
            editor_sessions: std::mem::take(&mut *self.docs.editor_sessions.write()),
            editor_views: std::mem::take(&mut *self.docs.editor_views.write()),
            active: *self.docs.active.peek(),
            folder: self.settings.folder.peek().clone(),
            workspace: self.settings.workspace.peek().clone(),
            closed: self.shell.closed_panels.peek().clone(),
            unavailable: self.saved_sessions.unavailable.peek().clone(),
        };
        let id = self.saved_sessions.active.peek().clone();
        self.saved_sessions.live.with_mut(|all| {
            all.insert(id, live);
        });
    }

    fn session_batch(&self, saved: &SavedWorkspace, active: &str) -> Batch {
        let key = Key::new().str(&saved.id);
        let batch = moonkale_state::put_in(Batch::new(), &key, saved);
        batch
            .put(
                "session_sources",
                key.into_bytes(),
                serde_json::to_vec(&saved.sources).unwrap(),
            )
            .put(
                "session_state",
                Key::new().str("active").into_bytes(),
                active.as_bytes().to_vec(),
            )
    }

    /// Write the current metadata off the input path.
    pub async fn persist_session(mut self) -> Result<(), String> {
        if !*self.saved_sessions.ready.peek() || *self.saved_sessions.switching.peek() {
            return Ok(());
        }
        let gate = self.saved_sessions.gate.peek().clone();
        let _guard = gate.lock().await;
        let mut saved = self.capture_session();
        if let Some(old) = self
            .saved_sessions
            .list
            .peek()
            .iter()
            .find(|s| s.id == saved.id)
        {
            saved.touched = old.touched;
            if saved == *old {
                return Ok(());
            }
            saved.touched = now_ms();
        }
        if let Some(store) = self.config.persistence.state {
            (store.write)(self.session_batch(&saved, &saved.id)).await?;
        }
        self.saved_sessions.list.with_mut(|all| {
            all.retain(|s| s.id != saved.id);
            all.push(saved);
        });
        Ok(())
    }

    /// Switching never closes a dirty document. Persistence must succeed first.
    pub async fn switch_session(mut self, id: String) -> Result<(), String> {
        if *self.saved_sessions.switching.peek() || *self.saved_sessions.active.peek() == id {
            return Ok(());
        }
        let saved = self
            .saved_sessions
            .list
            .peek()
            .iter()
            .find(|s| s.id == id && !s.archived)
            .cloned()
            .ok_or("unknown session")?;
        let gate = self.saved_sessions.gate.peek().clone();
        let _guard = gate.lock().await;
        // Unanswered startup recovery remains owned by this session when
        // its source-backed tabs are still clean. Switching is not discard.
        let pending: Vec<_> = Box::pin(self.recovery_snapshots())
            .await
            .into_iter()
            .filter(|snapshot| {
                self.docs.open.peek().iter().any(|(_, doc)| {
                    let doc = doc.peek();
                    doc.node.source.as_str() == snapshot.source
                        && doc.node.native_key == snapshot.document
                        && !doc.dirty()
                })
            })
            .collect();
        Box::pin(self.snapshot_documents(&pending)).await;
        let current = self.capture_session();
        if let Some(store) = self.config.persistence.state {
            (store.write)(self.session_batch(&current, &id)).await?;
        }
        self.saved_sessions.list.with_mut(|all| {
            all.retain(|s| s.id != current.id);
            all.push(current);
        });
        self.remember_live();
        self.saved_sessions.switching.set(true);
        self.stop_sharing();
        self.send(SessionMessage::WindowLeft {
            from: self.session.window.peek().clone(),
        });
        self.session.remote_tabs.set(Vec::new());
        self.session.local_tabs.set(Vec::new());
        self.session.peers.set(Vec::new());
        self.session.links_seen.set(Vec::new());
        self.session.foreign_drag.set(None);
        self.session.panel_offer.set(None);
        self.session.requested_move.set(None);
        self.session.own_drag.set(None);
        self.saved_sessions.active.set(id);
        Box::pin(self.restore_session(saved)).await;
        self.saved_sessions.switching.set(false);
        self.saved_sessions.generation.with_mut(|n| *n += 1);
        self.send(SessionMessage::Hello {
            from: self.session.window.peek().clone(),
        });
        Ok(())
    }

    async fn restore_session(mut self, saved: SavedWorkspace) {
        self.saved_sessions.suspended.set(false);
        self.saved_sessions.settings.set(saved.settings);
        let live = self
            .saved_sessions
            .live
            .with_mut(|all| all.remove(&saved.id));
        self.links.groups.set(Vec::new());
        self.links.views.set(Default::default());
        self.links.contents.set(Default::default());
        self.links.results.set(Default::default());
        self.links.focused.set(None);
        self.docs.open.set(Vec::new());
        self.docs.views.set(Vec::new());
        self.docs.editor_choice.set(Default::default());
        self.docs.editor_sessions.set(Default::default());
        self.docs.editor_views.set(Default::default());
        self.docs.graph_request.set(None);
        self.docs.reveal.set(None);
        self.docs.cursor.set(None);
        self.docs.selection.set(None);
        self.docs.cursor_line.set(None);
        self.docs.active.set(None);
        self.sources.open.set(Vec::new());
        self.settings.folder.set(None);
        self.settings.workspace.set(SettingsFile::new());
        self.saved_sessions.unavailable.set(Default::default());
        self.shell.closed_panels.set(Default::default());
        self.session.peers.set(Vec::new());
        self.session.own_drag.set(None);
        self.session.foreign_drag.set(None);
        self.session.sharing.set(crate::sharing::Sharing::default());
        self.session.presence_link.set(None);
        self.session.presence.set(Vec::new());
        let live_links = live.is_some();
        if let Some(live) = live {
            self.links.groups.set(live.link_groups);
            self.links.views.set(live.link_views);
            self.links.contents.set(live.link_contents);
            self.links.results.set(live.link_results);
            self.links.reveals.set(live.link_reveals);
            self.links.agent_documents.set(live.agent_documents);
            self.sources.open.set(live.sources);
            self.docs.open.set(live.docs);
            self.docs.views.set(live.views);
            self.docs.editor_choice.set(live.editor_choice);
            self.docs.editor_sessions.set(live.editor_sessions);
            self.docs.editor_views.set(live.editor_views);
            self.docs.active.set(live.active);
            self.settings.folder.set(live.folder);
            self.settings.workspace.set(live.workspace);
            self.shell.closed_panels.set(live.closed);
            self.saved_sessions.unavailable.set(live.unavailable);
        } else {
            for descriptor in saved.sources {
                let result = if let Some(path) = descriptor.id.as_str().strip_prefix("folder:") {
                    Box::pin(self.open_folder(path.to_string()))
                        .await
                        .map(|_| ())
                } else {
                    Box::pin(self.attach_source(descriptor.clone())).await
                };
                if let Err(e) = result {
                    self.saved_sessions.unavailable.with_mut(|ids| {
                        ids.insert(descriptor.id.clone());
                    });
                    self.set_status(format!("{}: {e}", descriptor.display_name));
                    // Retain unavailable members in the saved record.
                }
            }
            saved.layout.apply_to(&mut self.settings.workspace.write());
            for (source, path) in saved.documents {
                if let Some(node) = self.node_at_path(&source, &path).await {
                    let _ = Box::pin(self.open_node(node)).await;
                }
            }
            if let Some((source, path)) = saved.active {
                let id = self.docs.open.peek().iter().find_map(|(id, d)| {
                    let d = d.peek();
                    (d.node.source == source && d.node.native_key == path).then_some(*id)
                });
                let id = id.or_else(|| {
                    self.docs
                        .views
                        .peek()
                        .iter()
                        .find(|node| node.source == source && node.native_key == path)
                        .map(|node| node.id)
                });
                self.docs.active.set(id);
            }
        }
        if !live_links {
            self.links.groups.set(saved.link_groups);
            self.links.views.set(saved.link_views);
            self.links.contents.set(saved.link_contents);
            self.links.reveals.set(BTreeMap::new());
            self.links.agent_documents.set(Default::default());
        }
        let folder = self.settings.folder.peek().clone();
        if let Some(folder) = folder {
            Box::pin(self.load_history(&folder)).await;
            self.join_presence(folder.as_str());
        }
        self.resolve_settings();
        self.sources.graph_epoch.with_mut(|n| *n += 1);
    }

    /// Create an empty named session and switch to it.
    pub async fn new_session(mut self, name: String) -> Result<String, String> {
        validate_name(&name)?;
        let id = uuid::Uuid::new_v4().to_string();
        let saved = SavedWorkspace::empty(id.clone(), name.trim().into());
        if let Some(store) = self.config.persistence.state {
            (store.write)(moonkale_state::put_in(
                Batch::new(),
                &Key::new().str(&id),
                &saved,
            ))
            .await?;
        }
        self.saved_sessions.list.with_mut(|all| all.push(saved));
        Box::pin(self.switch_session(id.clone())).await?;
        Ok(id)
    }

    /// Change a session name without changing its identity.
    pub async fn rename_session(mut self, id: &str, name: String) -> Result<(), String> {
        let gate = self.saved_sessions.gate.peek().clone();
        let _guard = gate.lock().await;
        validate_name(&name)?;
        let mut saved = self
            .saved_sessions
            .list
            .peek()
            .iter()
            .find(|s| s.id == id)
            .cloned()
            .ok_or("unknown session")?;
        saved.name = name.trim().into();
        if let Some(store) = self.config.persistence.state {
            (store.write)(moonkale_state::put_in(
                Batch::new(),
                &Key::new().str(id),
                &saved,
            ))
            .await?;
        }
        self.saved_sessions.list.with_mut(|all| {
            all.retain(|s| s.id != id);
            all.push(saved);
        });
        Ok(())
    }

    /// Soft deletion retains all state. No Source write is performed.
    pub async fn delete_session(mut self, id: &str) -> Result<(), String> {
        if id == DEFAULT_SESSION {
            return Err("Default cannot be deleted".into());
        }
        if self.saved_sessions.active.peek().as_str() == id {
            Box::pin(self.switch_session(DEFAULT_SESSION.into())).await?;
        }
        let mut saved = self
            .saved_sessions
            .list
            .peek()
            .iter()
            .find(|s| s.id == id)
            .cloned()
            .ok_or("unknown session")?;
        saved.archived = true;
        if let Some(store) = self.config.persistence.state {
            (store.write)(moonkale_state::put_in(
                Batch::new(),
                &Key::new().str(id),
                &saved,
            ))
            .await?;
        }
        self.saved_sessions.list.with_mut(|all| {
            all.retain(|s| s.id != id);
            all.push(saved);
        });
        Ok(())
    }

    /// Export metadata only: recovery bodies and conversations stay private.
    pub fn export_session(&self) -> Result<String, String> {
        let saved = self.capture_session();
        let mut value = serde_json::to_value(&saved).map_err(|e| e.to_string())?;
        value["bundle_version"] = serde_json::json!(1);
        let text = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
        if text.len() > MAX_BUNDLE {
            return Err("session bundle too large".into());
        }
        Ok(text)
    }

    /// Import bounded metadata as a new, unopened session.
    pub async fn import_session(mut self, text: &str) -> Result<String, String> {
        let gate = self.saved_sessions.gate.peek().clone();
        let _guard = gate.lock().await;
        if text.len() > MAX_BUNDLE {
            return Err("session bundle too large".into());
        }
        let mut saved: SavedWorkspace = moonkale_state::session_bundle::parse(text)?;
        validate_name(&saved.name)?;
        saved.validate_links()?;
        // Imported content cannot grant permissions or automatic tool writes.
        saved.settings = saved.settings.without_authority().0;
        if saved.sources.len() > 256 || saved.documents.len() > 4096 {
            return Err("session bundle limits exceeded".into());
        }
        // Unknown JSON fields cannot smuggle secret values: SettingsFile is a
        // typed shape, containing only secret references. Import does not open
        // sources or apply settings until the user explicitly switches.
        saved.id = uuid::Uuid::new_v4().to_string();
        saved.archived = false;
        let id = saved.id.clone();
        if let Some(store) = self.config.persistence.state {
            (store.write)(moonkale_state::put_in(
                Batch::new(),
                &Key::new().str(&id),
                &saved,
            ))
            .await?;
        }
        self.saved_sessions.list.with_mut(|all| all.push(saved));
        Ok(id)
    }
}
fn validate_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() || name.len() > 128 || name.chars().any(char::is_control) {
        Err("invalid session name".into())
    } else {
        Ok(())
    }
}
