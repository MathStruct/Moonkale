//! `Stylesheet` — a `<link rel=stylesheet>` that survives the platforms
//! where dioxus's head insertion is lost (Milestone 9, P-087: the Android
//! WebView drops head elements created during the first render). It
//! inserts through an idempotent eval after mount and again whenever the
//! frame bumps `Workspace::assets_epoch` (its own `onmounted`).

use crate::Workspace;
use dioxus::prelude::*;

#[component]
pub fn Stylesheet(href: Asset) -> Element {
    let ws = use_context::<Workspace>();
    let url = href.to_string();
    use_effect(move || {
        let _ = ws.assets_epoch.read();
        let js = format!(
            r#"(() => {{ const h = {url:?}; if (![...document.querySelectorAll('link[rel=stylesheet]')].some(l => l.getAttribute('href') === h)) {{ const l = document.createElement('link'); l.rel = 'stylesheet'; l.href = h; document.head.appendChild(l); }} }})();"#
        );
        let _ = dioxus::document::eval(&js);
    });
    rsx! {}
}
