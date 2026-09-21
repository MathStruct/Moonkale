//! # moonkale-editor-terminal-native
//!
//! The first JavaScript-free twin of a JS editor ([[JavaScript Inventory]],
//! Milestone 12): a terminal panel that renders a [`vt100`] screen as
//! Dioxus rows of styled spans, turns key events into bytes in Rust, and
//! uses the same backends (`SpawnTerminal`: a PTY on desktop and on the
//! server, a websocket on the web) as the xterm.js panel. Opt-in until it
//! matches xterm.js; both can be enabled, and *New Terminal* then asks
//! which one to use (`terminal.implementation`).

mod keys;
mod panel;

pub use panel::{NativeTerminalExtension, PANEL_ID};

/// An async pause for the browser and desktop alike (the re-measure timer).
pub async fn sleep_ms(ms: u32) {
    #[cfg(target_arch = "wasm32")]
    gloo_timers::future::TimeoutFuture::new(ms).await;
    #[cfg(not(target_arch = "wasm32"))]
    tokio::time::sleep(std::time::Duration::from_millis(ms as u64)).await;
}
