//! Startup recovery. The store contains only a pending bit and bounded count.
pub use moonkale_ext_api::crash::StartupStore;
#[cfg(not(target_arch = "wasm32"))]
static STARTUP: std::sync::OnceLock<moonkale_ext_api::crash::StartupTracker> =
    std::sync::OnceLock::new();

/// Call before launching the native UI; never touches preferences or layout.
#[cfg(not(target_arch = "wasm32"))]
pub fn native_startup(store: Option<moonkale_ext_api::crash::StartupStore>) {
    if std::env::args().any(|a| a == "--safe") {
        std::env::set_var("MOONKALE_SAFE", "1");
    }
    if let Some(store) = store {
        let _ = STARTUP.set(moonkale_ext_api::crash::StartupTracker::begin(store));
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn healthy() {
    if let Some(tracker) = STARTUP.get() {
        tracker.healthy();
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn offer_safe() -> bool {
    STARTUP.get().is_some_and(|tracker| tracker.offer)
}

/// Installed before wasm launch. Only fixed strings/counts are persisted or
/// copied; neither panic payloads nor browser error objects are read.
#[cfg(target_arch = "wasm32")]
pub fn install_web() {
    let lang = js_sys::eval("navigator.language")
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_else(|| "en".into());
    web_labels(&lang);
    let _ = js_sys::eval(include_str!("../assets/crash.js"));
    std::panic::set_hook(Box::new(|_| {
        let _ = js_sys::eval("globalThis.moonkaleCrash?.()");
    }));
}

/// Read the already-installed startup query before constructing extensions.
#[cfg(target_arch = "wasm32")]
pub fn web_safe() -> bool {
    js_sys::eval("!!globalThis.moonkaleStartup?.safe")
        .ok()
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

/// Whether the last two client startups were incomplete.
#[cfg(target_arch = "wasm32")]
pub fn web_offer() -> bool {
    js_sys::eval("!!globalThis.moonkaleStartup?.offer")
        .ok()
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

/// Set translated overlay labels independently of the stopped wasm runtime.
#[cfg(target_arch = "wasm32")]
pub fn web_labels(lang: &str) {
    let words = [
        crate::tl!(lang, "crash-web-stopped"),
        crate::tl!(lang, "crash-web-reload"),
        crate::tl!(lang, "crash-web-copy"),
        crate::tl!(lang, "crash-safe-start"),
    ];
    let _ = js_sys::eval(&format!(
        "globalThis.moonkaleCrashWords = {};",
        serde_json::to_string(&words).unwrap_or_default()
    ));
}

/// Guard all host-to-extension entry points, including contribution discovery.
/// Component descendants receive the same identity from `ExtensionPanel`.
pub(crate) struct GuardedExtension {
    inner: Box<dyn moonkale_ext_api::Extension>,
    manifest: moonkale_ext_api::Manifest,
    guard: moonkale_ext_api::crash::ExtensionGuard,
}
impl GuardedExtension {
    pub fn wrap(
        inner: Box<dyn moonkale_ext_api::Extension>,
        ws: moonkale_ext_api::Workspace,
    ) -> Box<dyn moonkale_ext_api::Extension> {
        let manifest = inner.manifest();
        let guard = moonkale_ext_api::crash::ExtensionGuard::new(ws, manifest.id);
        Box::new(Self {
            inner,
            manifest,
            guard,
        })
    }
}
impl moonkale_ext_api::Extension for GuardedExtension {
    fn manifest(&self) -> moonkale_ext_api::Manifest {
        self.manifest.clone()
    }
    fn panels(&self, ws: moonkale_ext_api::Workspace) -> Vec<moonkale_ext_api::PanelContribution> {
        self.guard.run(|| self.inner.panels(ws)).unwrap_or_default()
    }
    fn render(&self, id: &str, ws: moonkale_ext_api::Workspace) -> dioxus::prelude::Element {
        self.guard
            .run(|| self.inner.render(id, ws))
            .unwrap_or_else(dioxus::prelude::VNode::empty)
    }
    fn preview(
        &self,
        id: &str,
        ws: moonkale_ext_api::Workspace,
        size: moonkale_ext_api::preview::PreviewSize,
    ) -> Option<dioxus::prelude::Element> {
        self.guard
            .run(|| self.inner.preview(id, ws, size))
            .flatten()
    }
    fn on_panel_closed(&self, id: &str, ws: moonkale_ext_api::Workspace) {
        self.guard.run(|| self.inner.on_panel_closed(id, ws));
    }
    fn commands(
        &self,
        ws: moonkale_ext_api::Workspace,
    ) -> Vec<moonkale_ext_api::CommandContribution> {
        self.guard
            .run(|| self.inner.commands(ws))
            .unwrap_or_default()
    }
    fn run_command(&self, id: &str, ws: moonkale_ext_api::Workspace) {
        self.guard.run(|| self.inner.run_command(id, ws));
    }
    fn claims(&self, node: &moonkale_core::Node) -> Option<u8> {
        self.guard.run(|| self.inner.claims(node)).flatten()
    }
    fn flow_libraries(&self) -> Vec<moonkale_ext_api::flow::FlowLibrary> {
        self.guard
            .run(|| self.inner.flow_libraries())
            .unwrap_or_default()
    }
    fn settings(
        &self,
        ws: moonkale_ext_api::Workspace,
        target: moonkale_ext_api::SettingsTarget,
    ) -> Option<dioxus::prelude::Element> {
        self.guard.run(|| self.inner.settings(ws, target)).flatten()
    }
    fn locales(&self) -> moonkale_ext_api::i18n::Locales {
        self.guard.run(|| self.inner.locales()).unwrap_or(&[])
    }
    fn themes(&self) -> Vec<String> {
        self.guard.run(|| self.inner.themes()).unwrap_or_default()
    }
}

#[cfg(all(feature = "crash-test", debug_assertions))]
pub(crate) fn test_requested() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::eval("new URLSearchParams(location.search).has('crash-test')")
            .ok()
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::env::var_os("MOONKALE_CRASH_TEST").is_some()
    }
}

#[cfg(all(feature = "crash-test", debug_assertions))]
pub(crate) struct TestExtension;
#[cfg(all(feature = "crash-test", debug_assertions))]
impl moonkale_ext_api::Extension for TestExtension {
    fn manifest(&self) -> moonkale_ext_api::Manifest {
        moonkale_ext_api::Manifest::core(
            "dev.moonkale.crash-test",
            "Crash test",
            "Test-only failure injection",
        )
    }
    fn panels(&self, _: moonkale_ext_api::Workspace) -> Vec<moonkale_ext_api::PanelContribution> {
        vec![moonkale_ext_api::PanelContribution::new(
            "crash-test",
            "Crash test",
            moonkale_ext_api::PanelHome::Side,
        )
        .activity(moonkale_ext_api::Activity::new(
            "warning",
            999,
            "Crash test",
        ))]
    }
    fn render(&self, _: &str, ws: moonkale_ext_api::Workspace) -> dioxus::prelude::Element {
        use dioxus::prelude::*;
        use moonkale_ext_api::{
            crash::{event, spawn, ExtensionGuard},
            t,
        };
        rsx! {
            div { id: "mk-crash-test",
                button { id: "mk-crash-test-event", onclick: event(move |_: MouseEvent| -> () { panic!("test-only event failure"); }), {t!(ws, crate::L, "crash-test-event")} }
                button { id: "mk-crash-test-task", onclick: event(move |_| { spawn(async move { panic!("test-only task failure"); }); }), {t!(ws, crate::L, "crash-test-task")} }
                button { id: "mk-crash-test-disable", onclick: event(move |_| ExtensionGuard::new(ws, "dev.moonkale.crash-test").failed()), {t!(ws, crate::L, "crash-test-disable")} }
            }
        }
    }
}
