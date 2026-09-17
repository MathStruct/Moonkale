//! # moonkale-editor-graph-desktop  (desktop only)
//!
//! The graph renderer in `moonkale-editor-graph` is written against `wgpu`
//! and draws into whatever `SurfaceProvider` the platform gives it. Inside a
//! webview the default provider is a `<canvas>` (WebGPU, or WebGL2 without
//! compute). On Linux the desktop webview is WebKitGTK, where WebGPU is not
//! reliably available, so this crate provides the **alternative desktop
//! implementation**: a native `wgpu` surface composited over the panel.
//!
//! Selected by the `desktop` crate's `graph-native` feature. The graph
//! *editor* (scene, layout, interaction, styles) is unchanged; only where
//! pixels land differs. See vault: `decisions/ADR-0011 Desktop graph surface
//! strategy.md`, `platform/Linux Desktop Setup.md`.
//!
//! ```text
//!   tao Window
//!   └─ GTK container
//!      ├─ WebKitGTK WebView   (the Dioxus UI; panel leaves a hole)
//!      └─ native surface      (wgpu; positioned/sized to the hole,
//!                              hidden when the panel is not visible)
//! ```

pub mod input;
pub mod overlay;
pub mod probe;
pub mod sync;
