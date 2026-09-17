//! Runtime probe: decide at startup which surface to use. Evaluate
//! `!!navigator.gpu` in the webview; if WebGPU is present prefer the in-webview
//! canvas (one code path); otherwise enable the overlay. Expose the result as
//! a `graphSurface` context key and in the "About / Diagnostics" panel so
//! users can see why they got which renderer.
