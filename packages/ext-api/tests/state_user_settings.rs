//! Phase 5.18: with `user_settings_in_state`, the user's settings live in
//! the state store (table `settings`, key `"user"`). What the platform's
//! settings file holds is imported on the first load; after that only the
//! store is written and read.

use dioxus::prelude::*;
use moonkale_core::SourceError;
use moonkale_ext_api::settings::{SettingsFile, UserSettingsRecord};
use moonkale_ext_api::{OpenFolderFuture, OpenOptions, SettingsFuture, Workspace, WorkspaceConfig};
use moonkale_state::{MemoryStore, Typed};
use std::sync::{Arc, Mutex};

/// The platform's settings file, in memory.
static FILE: Mutex<Option<String>> = Mutex::new(None);

fn load() -> SettingsFuture<SettingsFile> {
    Box::pin(async {
        match FILE.lock().unwrap().clone() {
            Some(text) => SettingsFile::parse(&text),
            None => Ok(SettingsFile::new()),
        }
    })
}

fn save(file: SettingsFile) -> SettingsFuture<()> {
    Box::pin(async move {
        *FILE.lock().unwrap() = Some(file.to_json());
        Ok(())
    })
}

fn open_folder(_: String, _: OpenOptions) -> OpenFolderFuture {
    Box::pin(async { Err(SourceError::NotFound) })
}

fn attach(_: moonkale_core::SourceDescriptor) -> moonkale_ext_api::AttachFuture {
    Box::pin(async { Err(SourceError::NotFound) })
}

thread_local! {
    static SETUP: std::cell::RefCell<Option<(Workspace, Arc<MemoryStore>)>> = const { std::cell::RefCell::new(None) };
}

#[component]
fn App() -> Element {
    let store = use_hook(|| Arc::new(MemoryStore::new()));
    let state = moonkale_ext_api::local_state(store.clone());
    let ws = use_context_provider(|| {
        Workspace::new(WorkspaceConfig {
            folders: moonkale_ext_api::FolderAccess {
                open: open_folder,
                pick: None,
                attach,
                reopen_last: false,
                openers: &moonkale_core::source::opener::NO_OPENERS,
            },
            processes: Default::default(),
            persistence: moonkale_ext_api::Persistence {
                settings: Some(moonkale_ext_api::SettingsStore { load, save }),
                state: Some(state),
                user_settings_in_state: true,
                ..Default::default()
            },
            network: Default::default(),
            runtimes: Default::default(),
        })
    });
    use_hook(move || SETUP.with(|s| *s.borrow_mut() = Some((ws, store))));
    rsx! { div {} }
}

async fn settle(dom: &mut VirtualDom) {
    for _ in 0..30 {
        tokio::select! {
            _ = dom.wait_for_work() => {}
            _ = tokio::time::sleep(std::time::Duration::from_millis(10)) => {}
        }
        dom.render_immediate(&mut dioxus_core::NoOpMutations);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn user_settings_move_into_the_store() {
    *FILE.lock().unwrap() = Some(r#"{"version":1,"user_name":"from-file"}"#.into());
    let mut dom = VirtualDom::new(App);
    dom.rebuild_in_place();
    settle(&mut dom).await;
    let (ws, store) = SETUP.with(|s| s.borrow_mut().take()).unwrap();
    let stored = |store: &MemoryStore| {
        Typed::new(store)
            .get::<UserSettingsRecord>(&UserSettingsRecord::key())
            .unwrap()
            .map(|r| r.0.user_name)
    };

    // First load: the file is imported.
    dom.in_scope(ScopeId::ROOT, || {
        spawn(async move { ws.load_user_settings().await })
    });
    settle(&mut dom).await;
    assert_eq!(
        ws.settings.user.peek().user_name.as_deref(),
        Some("from-file")
    );
    assert_eq!(stored(&store), Some(Some("from-file".into())));

    // A change goes to the store only.
    dom.in_scope(ScopeId::ROOT, || {
        spawn(async move {
            ws.update_user_settings(|f| f.user_name = Some("changed".into()))
                .await
        })
    });
    settle(&mut dom).await;
    assert_eq!(stored(&store), Some(Some("changed".into())));
    assert!(FILE
        .lock()
        .unwrap()
        .as_deref()
        .unwrap()
        .contains("from-file"));

    // The next start reads the store, whatever the old file says.
    *FILE.lock().unwrap() = Some(r#"{"version":1,"user_name":"edited-by-hand"}"#.into());
    let mut ws2 = ws;
    ws2.settings.user.set(SettingsFile::new());
    dom.in_scope(ScopeId::ROOT, || {
        spawn(async move { ws2.load_user_settings().await })
    });
    settle(&mut dom).await;
    assert_eq!(
        ws.settings.user.peek().user_name.as_deref(),
        Some("changed")
    );
}
