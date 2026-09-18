---
title: "Linux Desktop Setup"
tags: [platform, linux, desktop]
---
What Moonkale's desktop build needs from a Linux system, why Linux is the odd one out, and what to change on this machine. Facts below were checked on the dev box (Arch, kernel 7.2, **WebKitGTK 2.52.6**, NVIDIA RTX 3080 + AMD Raphael iGPU) on 2026-09-17.

## Why Linux is the way it is

Dioxus desktop does not ship a browser engine. It uses the platform's system webview through `wry`:

| OS | webview | engine | WebGPU |
|---|---|---|---|
| Windows | WebView2 | Chromium (Edge) | ✅ |
| macOS | WKWebView | WebKit (Safari) | ✅ recent macOS |
| **Linux** | **WebKitGTK** | WebKit, GTK port maintained by Igalia | ⚠️ not enabled by default |

Consequences:
1. **What works is decided by the distro's WebKitGTK package**, not by us. Arch ships current releases (2.52.x); Debian stable lags by years. The app must degrade gracefully.
2. **WebKitGTK's GPU path is fragile on NVIDIA.** It renders through DMA-BUF buffers and a compositing pipeline that the proprietary NVIDIA driver historically mishandles → blank/black windows, flicker, or crashes. Dioxus 0.7 already *disables DMA-BUF on Wayland by default* (`Config::with_disable_dma_buf_on_wayland`, default `true`) for exactly this reason.
3. **WebGPU in WebKitGTK exists but is behind a feature flag.** The Rust `webkit2gtk` bindings (2.0.1) do not expose `webkit_settings_set_feature_enabled`, so enabling it from Moonkale means a small raw-FFI call. Even enabled, it is experimental. This is the root of [[P-001 Graph surface in desktop webview]] and why [[ADR-0011 Desktop graph surface strategy]] plans a native fallback.
4. **Two GPUs** (NVIDIA dGPU + AMD iGPU) add a PRIME dimension: which GPU the webview and a native `wgpu` overlay each pick may differ.

## Packages (Arch)

```sh
# runtime + build deps for dioxus desktop (wry/WebKitGTK)
sudo pacman -S --needed webkit2gtk-4.1 gtk3 libappindicator-gtk3 xdotool
# xdotool is NOT optional: it provides libxdo.so, which `muda` (Dioxus's native
# menu crate) links on Linux. Without it the desktop build fails at link time
# with `rust-lld: error: unable to find library -lxdo` (P-038).

# native file/folder dialogs: rfd's xdg-portal backend needs a portal daemon
# plus one backend matching your desktop (gtk for GNOME/Xfce/most WMs, kde, hyprland…)
sudo pacman -S --needed xdg-desktop-portal xdg-desktop-portal-gtk
# present here: xdg-desktop-portal 1.22.1, -gtk 1.15.3, -kde 6.7.5
# already present here: webkit2gtk-4.1 2.52.6, webkitgtk-6.0 2.52.6

# native graph overlay plan (ADR-0011) + wgpu on Vulkan
sudo pacman -S --needed vulkan-icd-loader vulkan-tools   # `vulkaninfo --summary` to see which ICDs exist
# NVIDIA: nvidia-utils provides the Vulkan ICD; AMD iGPU: vulkan-radeon

# only if lbug's prebuilt liblbug download fails (it succeeded here, 2026-09-18):
sudo pacman -S --needed cmake gcc make

# NVIDIA (proprietary driver): WebKitGTK's DMA-BUF renderer crashes its web process in
# libnvidia-eglcore (P-061). Moonkale sets WEBKIT_DISABLE_DMABUF_RENDERER=1 itself when it
# sees /proc/driver/nvidia/version; to test the other way round:
#   MOONKALE_KEEP_DMABUF=1 dx serve --platform desktop
# Check for new dumps with: coredumpctl list WebKitWebProcess

# language servers (optional; the status bar tells you what is missing)
rustup component add rust-analyzer
```

Debian/Ubuntu equivalents: `libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev libxdo-dev`. Fedora: `webkit2gtk4.1-devel gtk3-devel libappindicator-gtk3-devel xdotool`.

## Environment variables you may need

| Variable | When | Effect |
|---|---|---|
| `WEBKIT_DISABLE_DMABUF_RENDERER=1` | blank/black window, NVIDIA, Wayland | Dioxus sets the equivalent by default on Wayland; set explicitly under X11 if the window is black |
| `WEBKIT_DISABLE_COMPOSITING_MODE=1` | still broken | disables accelerated compositing entirely: everything renders, WebGL is gone |
| `GDK_BACKEND=x11` | Wayland-specific glitches | forces XWayland for the app |
| `__NV_PRIME_RENDER_OFFLOAD=1 __GLX_VENDOR_LIBRARY_NAME=nvidia` | want the dGPU | PRIME offload for GL; for Vulkan use `VK_ICD_FILENAMES` / `MESA_VK_DEVICE_SELECT` |
| `WGPU_BACKEND=vulkan` / `WGPU_ADAPTER_NAME=NVIDIA` | native overlay | pick the wgpu backend/adapter |
| `RUST_LOG=info` `RUST_BACKTRACE=1` | debugging | see [[Debugging and Logging]] |

The dev box's session type was empty when checked (headless via code-server). Run the desktop app from a real Wayland/X11 session; under a headless session nothing will open.

## Checking what the webview can do

Run the desktop app in debug, open the inspector (menu **Toggle Developer Tools** or right-click → *Inspect Element*), and in the console:

```js
!!navigator.gpu                                          // WebGPU present?
navigator.gpu?.requestAdapter().then(a => console.log(a?.info ?? a))
!!document.createElement('canvas').getContext('webgl2')  // WebGL2 present?
```

Record the answers in [[P-001 Graph surface in desktop webview]] under *Measurements*. Moonkale will do this probe automatically at startup (`editors/graph-desktop/src/probe.rs`) and expose the result in a Diagnostics panel.

## Enabling WebGPU in WebKitGTK (experiment)

WebKitGTK ≥ 2.42 has a runtime feature API. Since the Rust bindings don't wrap it, the experiment is:

1. In `desktop/src/main.rs`, use `Config::with_on_window` (or after launch via `dioxus::desktop::window().webview`) and `wry::WebViewExtUnix::webview()` to reach the `webkit2gtk::WebView`.
2. Call the C API through `webkit2gtk-sys`: `webkit_settings_get_all_features()` → iterate, find the feature whose identifier is `"WebGPU"`, `webkit_settings_set_feature_enabled(settings, feature, TRUE)`.
3. Re-run the console probe.

If this works reliably on 2.52 with both GPUs, plan A (in-webview canvas) is enough on this machine and the overlay stays a fallback for older distros. If it doesn't, the overlay becomes the Linux default. Either way the outcome is an ADR-0011 measurement.

## Known WebKitGTK limitations that affect Moonkale
- No `SharedArrayBuffer` without COOP/COEP headers — the terminal's binary path uses base64 first ([[JS Interop Boundary]]).
- File System Access API: absent (Chromium-only) — irrelevant on desktop (we use `std::fs`) but matters for the web build in Epiphany.
- Devtools are compiled in for debug builds; for release builds enable wry's `devtools` feature explicitly ([[Debugging and Logging]]).
