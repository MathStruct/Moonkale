//! The native surface.
//!
//! Linux (GTK): a sibling widget in the same GTK container as the WebKitGTK
//! view (obtained through `wry::WebViewExtUnix::webview()` → parent), with a
//! `wgpu::Surface` created from its raw window handle. Windows (WebView2) and
//! macOS (WKWebView) can use WebGPU in the webview and normally don't need
//! this crate; the overlay is still implementable via a child HWND / NSView.
//!
//! Z-order with webview-drawn popups and menus is the known hard part: the
//! popup for a hovered node is drawn by the webview *under* the overlay.
//! Mitigations: draw popups natively (egui-style immediate UI on the same
//! surface), or cut the popup region out of the overlay.
