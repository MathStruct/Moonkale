//! One quiet recovery notice, with explicit per-document decisions.
use crate::L;
use dioxus::prelude::*;
use moonkale_ext_api::{t, Workspace};
use moonkale_state::recovery::Snapshot;

#[derive(Clone)]
enum Action {
    Tick,
    Restore(Snapshot),
    Discard(Snapshot),
}

#[component]
pub(crate) fn Recovery(ws: Workspace) -> Element {
    let mut offers = use_signal(Vec::<Snapshot>::new);
    let mut conflict = use_signal(|| None::<(Snapshot, String)>);
    let worker = use_coroutine(move |mut rx: UnboundedReceiver<Action>| async move {
        use futures_util::StreamExt;
        let mut pending = ws.recovery_snapshots().await;
        offers.set(pending.clone());
        while let Some(action) = rx.next().await {
            match action {
                Action::Tick => {
                    Box::pin(ws.snapshot_documents(&pending)).await;
                }
                Action::Restore(snapshot) => {
                    let source = moonkale_core::SourceId(snapshot.source.clone());
                    // The browser deliberately starts without a folder. A person's
                    // explicit Restore uses the existing, server-jailed open path.
                    #[cfg(target_arch = "wasm32")]
                    if ws.source(&source).is_none() {
                        if let Some(path) = snapshot.source.strip_prefix("folder:") {
                            let _ = Box::pin(ws.open_folder(path.to_string())).await;
                        }
                    }
                    if ws.source(&source).is_none() {
                        let mut ws = ws;
                        ws.set_status(t!(ws, L, "recovery-open-source"));
                        continue;
                    }
                    let admitted = if let Some(src) = ws.source(&source) {
                        src.query(moonkale_core::Query::Text {
                            dialect: "recovery-path".into(),
                            text: snapshot.document.clone(),
                        })
                        .await
                        .ok()
                        .and_then(|r| r.nodes.into_iter().next())
                    } else {
                        None
                    };
                    if let Some(node) = admitted {
                        if Box::pin(ws.open_node(node.clone())).await.is_ok() {
                            if let Some(mut doc) = ws.document(node.id) {
                                let current = doc.peek().clone();
                                if current.version.to_string() == snapshot.base_version
                                    && current.saved == snapshot.base
                                    && !current.dirty()
                                {
                                    doc.with_mut(|d| d.text = snapshot.text.clone());
                                    pending.retain(|s| s.key() != snapshot.key());
                                    offers.set(pending.clone());
                                } else {
                                    conflict.set(Some((snapshot, current.text)));
                                }
                            }
                        }
                    }
                }
                Action::Discard(snapshot) => {
                    ws.forget_recovery(&snapshot.source, &snapshot.document)
                        .await;
                    pending
                        .retain(|s| s.source != snapshot.source || s.document != snapshot.document);
                    offers.set(pending.clone());
                    conflict.set(None);
                }
            }
        }
    });
    use_effect(move || {
        let _ = *ws.docs.active.read();
        let _ = ws.sources.open.read().len();
        worker.send(Action::Tick);
    });
    use_effect(move || {
        let mut timer = document::eval(
            r#"
            const tick = () => dioxus.send(true);
            const timer = setInterval(tick, 2000);
            const blur = () => tick();
            window.addEventListener('blur', blur);
            document.addEventListener('visibilitychange', blur);
            tick();
            await new Promise(resolve => window.addEventListener('pagehide', resolve, {once:true}));
            clearInterval(timer);
            window.removeEventListener('blur', blur);
            document.removeEventListener('visibilitychange', blur);
        "#,
        );
        spawn(async move {
            while timer.recv::<bool>().await.is_ok() {
                worker.send(Action::Tick);
            }
        });
    });
    rsx! {
        if !offers.read().is_empty() {
            details { class: "mk-recovery",
                summary { {t!(ws, L, "recovery-notice", count = offers.read().len())} }
                for snapshot in offers.read().iter().cloned() {
                    div { key: "{snapshot.source}:{snapshot.document}",
                        span { "{snapshot.document}" }
                        button { onclick: { let s = snapshot.clone(); move |_| worker.send(Action::Restore(s.clone())) }, {t!(ws, L, "recovery-restore")} }
                        button { onclick: move |_| worker.send(Action::Discard(snapshot.clone())), {t!(ws, L, "recovery-discard")} }
                    }
                }
            }
        }
        if let Some((snapshot, current)) = conflict.read().as_ref() {
            div { class: "mk-recovery-conflict", role: "dialog", "aria-label": t!(ws, L, "recovery-conflict"),
                h3 { {t!(ws, L, "recovery-conflict")} }
                p { "{snapshot.document}" }
                h4 { {t!(ws, L, "recovery-base")} }
                pre { "{snapshot.base}" }
                h4 { {t!(ws, L, "recovery-current")} }
                pre { "{current}" }
                h4 { {t!(ws, L, "recovery-buffer")} }
                pre { "{snapshot.text}" }
                button { onclick: move |_| conflict.set(None), {t!(ws, L, "recovery-close")} }
            }
        }
    }
}
