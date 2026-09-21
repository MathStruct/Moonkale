# Development

The mobile crate defines the entrypoint for the mobile app along with any assets, components and dependencies that are specific to mobile builds. The mobile crate starts out something like this:

```
mobile/
├─ assets/ # Assets used by the mobile app - Any platform specific assets should go in this folder
├─ src/
│  ├─ main.rs # The entrypoint for the mobile app.It also defines the routes for the mobile platform
│  ├─ views/ # The views each route will render in the mobile version of the app
│  │  ├─ mod.rs # Defines the module for the views route and re-exports the components for each route
│  │  ├─ blog.rs # The component that will render at the /blog/:id route
│  │  ├─ home.rs # The component that will render at the / route
├─ Cargo.toml # The mobile crate's Cargo.toml - This should include all mobile specific dependencies
```

## Dependencies
Since you have fullstack enabled, the mobile crate will be built two times:
1. Once for the server build with the `server` feature enabled
2. Once for the client build with the `mobile` feature enabled

You should make all mobile specific dependencies optional and only enabled in the `mobile` feature. This will ensure that the server builds don't pull in mobile specific dependencies which cuts down on build times significantly.

### Serving Your Mobile App

Mobile platforms are shared in a single crate. To serve mobile, you need to explicitly set your target device to `android` or `ios`:

```bash
dx serve --platform android
```
## Moonkale (Milestones 6 and 9)

`packages/mobile/src/main.rs` mounts the same `ui::Frame` as desktop and web. Below 700 px the shell collapses to one tile with a bottom bar (`ui/src/shell.rs`), which is what a phone shows.

- `app_folder()` — the app's private files dir (`/data/data/io.github.mathstruct.moonkale/files`), seeded with a small `vault/` on first launch; `settings.json` lives next to it (`settings_store` in `WorkspaceConfig`, `reopen_last_folder: true`). User-chosen folders (Storage Access Framework) are not wired yet.
- Everything else is the shared code. Android-only behaviour is in the shared crates, keyed on the platform: `moonkale_ext_api::Stylesheet` (P-087), absolute asset URLs and `prefer: "gl"` in the Graph panel (P-088, P-089), the non-sRGB surface and re-fit on resize in `graph-render` (P-090, P-091).

### Build for a phone (verified on a Galaxy S10e, Android 13)

The short way (spec 007 — sets the app name and icon, which dx 0.7.10 does not, and reassembles):

```bash
packages/mobile/build-android.sh            # release arm64 APK with name + icon
packages/mobile/build-android.sh install    # … and adb install -r + launch
```

The long way, step by step:

```bash
export ANDROID_HOME=$HOME/Android/Sdk
export ANDROID_NDK_HOME=$ANDROID_HOME/ndk/29.0.14206865 NDK_HOME=$ANDROID_NDK_HOME
export JAVA_HOME=/usr/lib/jvm/java-17-openjdk PATH=$ANDROID_HOME/platform-tools:$PATH
cd packages/mobile
dx build --release --platform android --features mobile --target aarch64-linux-android
adb install -r ../../target/dx/mobile/release/android/app/app/build/outputs/apk/debug/app-debug.apk
adb shell monkey -p io.github.mathstruct.moonkale -c android.intent.category.LAUNCHER 1
```

Release, not debug: the debug APK is x86_64 by default (`INSTALL_FAILED_NO_MATCHING_ABIS`) and several times larger (`INSTALL_FAILED_INSUFFICIENT_STORAGE` on a phone with 1.4 GB free). The output path still says `debug` because the APK is unsigned; signing keys go in `Dioxus.toml` — never commit them.

### Looking inside without touching the phone

```bash
adb exec-out screencap -p > shot.png
PID=$(adb shell pidof io.github.mathstruct.moonkale)
adb forward tcp:9222 localabstract:webview_devtools_remote_$PID     # WebView DevTools
adb shell run-as io.github.mathstruct.moonkale cat files/vault/Home.md
```

With the forward in place any CDP client can evaluate JavaScript in the page — `node packages/web/tests/e2e/android-cdp.mjs '<expr>'` — and inject touch gestures: `android-pinch.mjs` pinches and rotates the graph through the WebView's `Input.dispatchTouchEvent` (spec 006). Logs: `adb logcat -s RustStdoutStderr chromium`.

## Milestone 12 — the phone as a server's client
`main()` installs `api::relay` before launch (P-098) and *File → Connect to Server…* (`ServerClient` over `api::client`) makes the app a client of a Moonkale server: `open_any`/`attach_any` route to the server while connected, `spawn_terminal` gives a shell *on the server* (the phone has none), `git_any` likewise, and the Agent panel uses **server sessions** (the phone has no local provider, so `agent_sessions()` needs no setting): a Claude Code turn started elsewhere keeps running there and the phone shows its state. Over the network the server should be reached through HTTPS (`MOONKALE_TLS_CERT/KEY`) or a tunnel; `MOONKALE_INSECURE_HTTP=1` on the server for a trusted LAN. Verified on the device 2026-09-22 against a LAN server (`server --bind 0.0.0.0 --port 8443 --token-stdin` with `MOONKALE_INSECURE_HTTP=1`): status `⇅ http://…:8443`, the server's folder next to the phone's vault, its files open. The first attempt did nothing — P-116, the dialog's task was cancelled with the dialog.
