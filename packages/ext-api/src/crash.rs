//! Extension failure containment. Guards catch unwinding panics, never retain payloads.
//! Abort builds (including today's web client) recover through the page overlay.
use crate::{Manifest, Workspace};
use dioxus::prelude::*;
use std::{collections::BTreeSet, future::Future, panic::AssertUnwindSafe};

#[derive(Clone, Copy)]
/// Session-only extension disablement and recovery notice.
pub struct CrashState {
    /// Extensions disabled after a failure.
    pub disabled: Signal<BTreeSet<String>>,
    /// Most recent failure, shown in one notice.
    pub notice: Signal<Option<String>>,
    /// Ignore extensions and saved layout for this session.
    pub safe: Signal<bool>,
}

impl CrashState {
    /// Allocate state in the root scope.
    pub fn new() -> Self {
        Self {
            disabled: Signal::new_in_scope(BTreeSet::new(), ScopeId::ROOT),
            notice: Signal::new_in_scope(None, ScopeId::ROOT),
            safe: Signal::new_in_scope(false, ScopeId::ROOT),
        }
    }

    /// Whether an extension may contribute in this session.
    pub fn allows(self, manifest: &Manifest) -> bool {
        !*self.safe.read() && !self.disabled.read().contains(manifest.id)
    }
}

impl Default for CrashState {
    fn default() -> Self {
        Self::new()
    }
}

/// One extension's identity, supplied by the shell to every panel subtree.
#[derive(Clone)]
pub struct ExtensionGuard {
    state: CrashState,
    id: String,
}

thread_local! {
    static ACTIVE: std::cell::RefCell<Option<ExtensionGuard>> = const { std::cell::RefCell::new(None) };
}
struct Active(Option<ExtensionGuard>);
impl Active {
    fn enter(guard: ExtensionGuard) -> Self {
        Self(ACTIVE.with(|active| active.replace(Some(guard))))
    }
}
impl Drop for Active {
    fn drop(&mut self) {
        ACTIVE.with(|active| active.replace(self.0.take()));
    }
}
fn current() -> Option<ExtensionGuard> {
    ACTIVE
        .with(|active| active.borrow().clone())
        .or_else(try_consume_context::<ExtensionGuard>)
}

/// Whether a synchronous entry point or a future poll is currently guarded.
/// Panic hooks use this to avoid logging or retaining extension panic payloads.
pub fn is_guarded() -> bool {
    ACTIVE
        .try_with(|active| active.try_borrow().is_ok_and(|active| active.is_some()))
        .unwrap_or(false)
}

impl ExtensionGuard {
    /// Bind the guard to one workspace and extension id.
    pub fn new(ws: Workspace, id: impl Into<String>) -> Self {
        Self {
            state: ws.crashes,
            id: id.into(),
        }
    }

    /// Disable this extension and replace the visible notice.
    pub fn failed(&self) {
        let mut state = self.state;
        state.disabled.with_mut(|ids| {
            ids.insert(self.id.clone());
        });
        state.notice.set(Some(self.id.clone()));
    }

    /// Whether this extension can still execute.
    pub fn enabled(&self) -> bool {
        !*self.state.safe.peek() && !self.state.disabled.peek().contains(&self.id)
    }

    /// Run a synchronous extension entry point. Payloads are discarded.
    pub fn run<T>(&self, f: impl FnOnce() -> T) -> Option<T> {
        if !self.enabled() {
            return None;
        }
        let _active = Active::enter(self.clone());
        match std::panic::catch_unwind(AssertUnwindSafe(f)) {
            Ok(value) => Some(value),
            Err(_) => {
                self.failed();
                None
            }
        }
    }

    /// Guard every poll, including polls after an await, and cancel siblings
    /// on their next poll. Dropping the panel cancels its scoped tasks too.
    pub async fn task(&self, future: impl Future<Output = ()>) {
        self.future(future).await;
    }

    /// Poll a future under the guard, returning no value after failure.
    pub async fn future<T>(&self, future: impl Future<Output = T>) -> Option<T> {
        let mut future = std::pin::pin!(future);
        std::future::poll_fn(|cx| {
            if !self.enabled() {
                return std::task::Poll::Ready(None);
            }
            let _active = Active::enter(self.clone());
            match std::panic::catch_unwind(AssertUnwindSafe(|| future.as_mut().poll(cx))) {
                Ok(poll) => poll.map(Some),
                Err(_) => {
                    self.failed();
                    std::task::Poll::Ready(None)
                }
            }
        })
        .await
    }
}

/// Use instead of Dioxus `spawn` in extension components. The shell supplies
/// the guard through context; outside an extension this behaves like Dioxus.
pub fn spawn(future: impl Future<Output = ()> + 'static) -> dioxus::core::Task {
    let guard = current();
    dioxus::prelude::spawn(async move {
        if let Some(guard) = guard {
            guard.task(future).await;
        } else {
            future.await;
        }
    })
}

/// A session service that survives panel unmounts, with the same failure guard.
pub fn spawn_forever(future: impl Future<Output = ()> + 'static) -> dioxus::core::Task {
    let guard = current();
    dioxus::core::spawn_forever(async move {
        if let Some(guard) = guard {
            guard.task(future).await;
        } else {
            future.await;
        }
    })
}

/// An effect belongs to the extension that registered it.
pub fn use_effect(mut callback: impl FnMut() + 'static) -> Effect {
    let guard = current();
    dioxus::prelude::use_effect(move || {
        if let Some(guard) = &guard {
            guard.run(&mut callback);
        } else {
            callback();
        }
    })
}

/// Guard construction and every poll of a resource. Failure disables the
/// owner; the shell unmounts its panels and drops the pending resource.
pub fn use_resource<T: 'static, F: Future<Output = T> + 'static>(
    mut factory: impl FnMut() -> F + 'static,
) -> Resource<T> {
    let guard = current();
    dioxus::prelude::use_resource(move || {
        let future = if let Some(guard) = &guard {
            guard.run(&mut factory)
        } else {
            Some(factory())
        };
        let guard = guard.clone();
        async move {
            if let Some(future) = future {
                match guard {
                    Some(guard) => {
                        if let Some(value) = guard.future(future).await {
                            return value;
                        }
                    }
                    None => return future.await,
                }
            }
            std::future::pending().await
        }
    })
}

/// Guard a component's background future, including construction.
pub fn use_future<F: Future<Output = ()> + 'static>(
    mut factory: impl FnMut() -> F + 'static,
) -> UseFuture {
    let guard = current();
    dioxus::prelude::use_future(move || {
        let future = if let Some(guard) = &guard {
            guard.run(&mut factory)
        } else {
            Some(factory())
        };
        let guard = guard.clone();
        async move {
            if let Some(future) = future {
                if let Some(guard) = guard {
                    guard.task(future).await;
                } else {
                    future.await;
                }
            }
        }
    })
}

/// Dispatch a guarded callback's return value. The marker distinguishes
/// synchronous callbacks from futures without overlapping implementations.
pub trait EventReturn<Marker> {
    /// Start any asynchronous work under the callback's captured owner.
    fn dispatch(self, guard: Option<ExtensionGuard>);
}
impl EventReturn<()> for () {
    fn dispatch(self, _: Option<ExtensionGuard>) {}
}
impl EventReturn<()> for dioxus::core::Task {
    fn dispatch(self, _: Option<ExtensionGuard>) {}
}
/// Type marker for an asynchronous event callback.
#[doc(hidden)]
pub enum AsyncEvent {}
impl<F: Future<Output = ()> + 'static> EventReturn<AsyncEvent> for F {
    fn dispatch(self, guard: Option<ExtensionGuard>) {
        use futures_util::FutureExt;
        let mut future = Box::pin(async move {
            if let Some(guard) = guard {
                guard.task(self).await;
            } else {
                self.await;
            }
        });
        // Match Dioxus: poll once inline so prevent_default works before the
        // browser completes its event, then schedule subsequent polls.
        if future.as_mut().now_or_never().is_none() {
            dioxus::prelude::spawn(future);
        }
    }
}

/// Wrap synchronous or async event callbacks. Capture ownership when the
/// callback is created, including callbacks made directly by an entry point.
pub fn event<A, R: EventReturn<Marker>, Marker>(
    mut callback: impl FnMut(A) -> R + 'static,
) -> impl FnMut(A) {
    let guard = current();
    move |argument| {
        let result = match &guard {
            Some(guard) => guard.run(|| callback(argument)),
            None => Some(callback(argument)),
        };
        if let Some(result) = result {
            result.dispatch(guard.clone());
        }
    }
}

/// Bounded startup failures. A completed startup clears the streak; a prior
/// pending startup counts once on the next launch, including hard termination.
#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct StartupCounter {
    /// Last launch has not reached its healthy checkpoint.
    pub pending: bool,
    /// Consecutive incomplete launches, saturating at 255.
    pub failures: u8,
}
impl StartupCounter {
    /// Begin a launch and report whether safe mode should be offered.
    pub fn begin(&mut self) -> bool {
        if self.pending {
            self.failures = self.failures.saturating_add(1);
        }
        self.pending = true;
        self.failures >= 2
    }
    /// Clear the failure streak once startup has settled.
    pub fn healthy(&mut self) {
        self.pending = false;
        self.failures = 0;
    }
}

/// Platform's own state store, never a workspace's or a remote server's.
pub type StartupStore = std::sync::Arc<dyn moonkale_state::StateStore>;

/// Durable native startup marker in the existing state database (ADR-0014).
pub struct StartupTracker {
    store: StartupStore,
    /// Two consecutive incomplete starts suggest safe mode.
    pub offer: bool,
}
impl StartupTracker {
    /// Record startup before launching UI or extensions.
    pub fn begin(store: StartupStore) -> Self {
        let mut counter: StartupCounter = store
            .get("health", b"startup")
            .ok()
            .flatten()
            .filter(|bytes| bytes.len() <= 128)
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        let offer = counter.begin();
        let _ = store.write(moonkale_state::Batch::new().put(
            "health",
            b"startup".to_vec(),
            serde_json::to_vec(&counter).unwrap_or_default(),
        ));
        Self { store, offer }
    }
    /// Mark a settled launch healthy without changing user settings.
    pub fn healthy(&self) {
        let _ = self.store.write(moonkale_state::Batch::new().put(
            "health",
            b"startup".to_vec(),
            b"{\"pending\":false,\"failures\":0}".to_vec(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_incomplete_startups_offer_safe_mode() {
        let mut counter = StartupCounter::default();
        assert!(!counter.begin());
        assert!(!counter.begin());
        assert!(counter.begin());
        for _ in 0..300 {
            counter.begin();
        }
        assert_eq!(counter.failures, u8::MAX);
        counter.healthy();
        assert!(!counter.begin());
    }
    #[test]
    fn startup_marker_roundtrips_in_the_existing_store_without_touching_settings() {
        use moonkale_state::StateStore;
        let store = std::sync::Arc::new(moonkale_state::MemoryStore::default());
        store
            .write(moonkale_state::Batch::new().put("settings", b"user".to_vec(), b"keep".to_vec()))
            .unwrap();
        assert!(!StartupTracker::begin(store.clone()).offer);
        assert!(!StartupTracker::begin(store.clone()).offer);
        let tracker = StartupTracker::begin(store.clone());
        assert!(tracker.offer);
        assert_eq!(
            store.get("settings", b"user").unwrap().as_deref(),
            Some(b"keep".as_slice())
        );
        tracker.healthy();
        assert!(!StartupTracker::begin(store.clone()).offer);
        store
            .write(moonkale_state::Batch::new().put("health", b"startup".to_vec(), vec![b'x'; 129]))
            .unwrap();
        assert!(!StartupTracker::begin(store).offer);
    }
}
