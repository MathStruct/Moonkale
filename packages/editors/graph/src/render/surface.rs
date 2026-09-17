//! Where the pixels go — the platform problem.
//!
//! | platform | surface strategy                                              |
//! |----------|---------------------------------------------------------------|
//! | web      | `<canvas>` + WebGPU via `wgpu` on wasm; WebGL2 fallback       |
//! | desktop  | the app is a *webview*; three options, in order of preference:|
//! |          | (a) same as web: run the wasm renderer inside the webview      |
//! |          |     (WebGPU availability varies: WebView2/WKWebView yes,        |
//! |          |     WebKitGTK partial); (b) native child window / overlay       |
//! |          |     rendered by wgpu, composited over the webview; (c) offscreen |
//! |          |     wgpu → shared texture/stream into a `<canvas>` (last resort)|
//! | mobile   | `<canvas>` in the webview (WebGL2 more reliable than WebGPU)   |
//!
//! The renderer is written against `wgpu` only; this module holds the
//! `trait SurfaceProvider` and the per-platform impls. Decision and
//! measurements are tracked in the vault ADR-0003.
