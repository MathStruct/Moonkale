---
title: "Android — extensions and bundling"
description: What an Android build of Moonkale contains, what can be added to it after install (and from where), how small the app could get, and what Google Play and F-Droid would say about each of those.
tags: [packaging, android, extensions]
---
Answers to [[Prompt8|three questions]]: can an extension come from a GitHub repository and work on Android without being bundled; can the app be small and extended; what do the stores allow. Background: [[Android]] (building), [[Extension System]], [[Publishing and Platforms]]. Numbers below are from the Milestone 9 build on the Galaxy S10e (2026-09-19).

## 1. What "bundled" means on Android

An APK is a zip. Ours holds three kinds of things, and only the first two are fixed at build time:

| part | in the 2026-09-19 APK (compressed) | can change after install? |
|---|---|---|
| `lib/arm64-v8a/libmain.so` — the whole Rust program: shell, every built-in editor and panel, the index, the static extensions (`ui::default_extensions()`: git, wordcount, …), SQLite/LadybugDB drivers | 14.9 MB (46 MB unpacked, **unstripped**) | **no** — new native code means a new APK |
| `assets/` — the JS packages (`milkdown.js` 1.0 MB, `codemirror.js` 0.14 MB, `xterm.js` 0.10 MB), the graph renderer wasm (0.97 MB), CSS, `wasm_host.js` | ≈ 3 MB, plus stale copies (see P-092) | no — the WebView loads them from the APK |
| `classes.dex` etc. — dx's Kotlin activity + AndroidX | 4.6 MB | no |
| **data the app downloads or the user puts in `files/`** — vault, settings, and *wasm extension modules* | 0 | **yes** |

So the honest split is: **Rust built-ins are static, wasm extensions are dynamic.** That is the same on every platform ([[Extension System]] "Runtimes by platform"); Android only makes it more visible because there is no `cargo build` on the phone.

## 2. An extension from GitHub, on Android, without rebuilding

**Yes, for wasm extensions (JSON ABI v1)** — with two pieces of work that are not done yet:

1. **A runtime on the phone.** Today `moonkale-ext-host` runs modules with wasmtime on desktop and the web server, and the browser runtime (`wasm_host.js`, a Worker + `SharedArrayBuffer` mailbox) runs them in the web client. The mobile crate passes `wasm: None, wasm_module_url: None`, so nothing runs on Android yet. Two ways to close that:
   - **The browser runtime inside the WebView** (preferred: it exists, and it is what the stores treat as "script in a WebView"). Needs the page to be *cross-origin isolated* for `SharedArrayBuffer` — on desktop/mobile the page comes from dioxus's custom protocol, whose responses we control, so COOP/COEP headers can be set there like `api::auth::protect` sets them on the server — and a way to hand the bytes to the page: a `use_asset_handler` route (`moonkale-ext://module/<id>`) that reads `files/extensions/<id>/<name>.wasm`. `Workspace::run_wasm_in_browser` already takes `wasm_module_url` from the config; the mobile crate would set it to that route.
   - **A native runtime**: wasmtime *can* target `aarch64-linux-android` (Cranelift has the backend) but it is not a tier the wasmtime project tests, and it is a JIT — which matters for Play (below). `wasmi` (a pure-Rust interpreter) is the safer choice for a phone: small, no JIT, no_std-friendly; it would be a second feature on `ext-host` behind the same `Runtime` API. Slower, but wordcount-sized tools do not care.
2. **An installer.** `discover(config_dir, folder)` already lists `<config>/extensions/*.wasm` and `<folder>/.moonkale/extensions/*.wasm`; on Android `config_dir` is the app's `files/`. "Install from GitHub" is then: fetch a release asset (`https://github.com/<owner>/<repo>/releases/download/<tag>/<id>.wasm` + `moonkale.toml`) over the `INTERNET` permission the manifest already grants, verify the hash the manifest carries (or a signature once the registry exists — [[Publishing and Platforms]] says that decision is pending), write it under `files/extensions/<id>/`, and rescan. The Settings → Extensions panel gets an "Install from URL" field; the same code serves desktop (`moonkale ext install <url>` is already the documented mechanism there, not yet implemented either).

What such an extension can do is bounded by **ABI v1**: commands and agent tools with `list_sources` / `query` / `fetch_text` under per-call permissions. It cannot contribute a panel, an editor, a language or a data source — those are static Rust contributions. A GitHub-hosted "Typst preview" or "Julia flow blocks" extension is therefore **not** possible without either (a) growing the wasm ABI to UI contributions (the `ui::Tree` panels planned since [[ADR-0004 WASM components for extensions]]), or (b) shipping it in the APK.

**No, for anything native.** A `.so` downloaded from GitHub is technically loadable (Android allows `dlopen` from app-private storage; only `exec` of downloaded files is blocked since API 29) but Google Play forbids it outright and F-Droid would not build it; we should not design for it.

## 3. A small app that is extended

Where the size goes and what would move it, in order of payoff:

| lever | effect | cost |
|---|---|---|
| **Strip and size-optimise the Android build** — `strip = true` (`llvm-strip` alone takes `libmain.so` from 46 MB to 27 MB, 10 MB gzipped), `opt-level = "s"`, `lto = "fat"`, `codegen-units = 1`, `panic = "abort"` in a mobile profile | APK ≈ 23 MB → ≈ 15 MB, nothing removed | one `[profile.mobile-release]`; symbols gone from panic backtraces on the phone (log the panic message, P-032 style) |
| **Clean the dx Android project before a release build** (P-092: every rebuild leaves the previous hashed asset in `app/src/main/assets`, five renderer wasm copies = 4 MB compressed) | −4 MB today, unbounded otherwise | `rm -rf target/dx/mobile/release/android/app/app/src/main/assets` or `dx clean` first |
| **Per-platform cargo features for built-in panels** — the editors are already separate crates registered in `ui::default_extensions()`; a `mobile` build could drop the terminal (no PTY on Android anyway), Typst, LSP, flow, the SQL drivers | a few MB each, and their JS assets | feature flags in `ui` and the catalog; the phone shell must not show tiles for absent panels |
| **JS packages off the APK** (`milkdown.js` is the largest asset) — lazy-load them from a CDN or the user's own server | −1.3 MB | offline first launch breaks; Play is fine with it, F-Droid flags network-loaded code |
| **Google Play feature delivery** — on-demand modules in an AAB | any built-in can become an install-time or on-demand module | Play-only; dx does not generate dynamic-feature modules, so this is a Gradle project we maintain by hand |
| **Everything optional as wasm extensions** | the only *store-neutral* way to a small core | needs ABI v2 with UI contributions first (see §2) |

A realistic "small core" after the first three rows is **≈ 12 MB**: shell, Explorer, code and markdown editors, graph, index, search, agent — with git, wordcount and LadybugDB compiled in because they are Rust. Below that, the APK is mostly the WebView bridge (`classes.dex`, 4.6 MB) and the JS editors.

## 4. Play Store and F-Droid

| | Google Play | F-Droid |
|---|---|---|
| package | AAB (`dx bundle --package-types aab`), signed with an upload key, `target_sdk` per current policy | they build the APK themselves from a git tag, reproducibly, with a `metadata/io.github.mathstruct.moonkale.yml` recipe |
| **downloaded wasm extensions** | allowed if they run "in a virtual machine or interpreter that provides indirect access to Android APIs (such as JavaScript in a WebView or browser)" — the policy's own exemption. The WebView runtime is exactly that; `wasmi` fits the wording; a wasmtime JIT is arguable. The app must not download `.so`/dex, and must not update *itself* outside Play | no policy against it, but they add an anti-feature note if the extensions themselves could be non-free; hashes/signatures make reviewers happier |
| **bundled JS packages** (`codemirror.js`, `milkdown.js`, `xterm.js` are prebuilt files in the repo) | fine | the recipe must **build them from source** (`npm ci && npm run build` in `packages/js/*` is allowed in `build:` steps) — prebuilt blobs in the repo are a review question, not a blocker, as long as they are rebuilt |
| **git dependencies** (a future embedded Helix, [[002]]) | fine | fine if the source is public; pinned revs preferred |
| **NDK build** | dx does it | supported via `ndk:` in the recipe; `rustup` targets via `srclibs` |
| what is missing today | signing config in `Dioxus.toml` (keys never committed), an AAB, a privacy declaration (the app talks to the LLM provider the user configures — must be disclosed), `target_sdk` bump | a signed release tag, the recipe, an icon (have it), a changelog (`fastlane/metadata` layout), no proprietary dependency (none today) |

Neither store restricts the *vault* or *settings* — data the user creates is never the store's concern.

## Recommendation
1. Now: the two size rows that cost nothing (mobile release profile with `strip`, clean assets before a release build) → P-092 and a `[profile.mobile-release]`.
2. When extensions on the phone are wanted: the WebView runtime with a `use_asset_handler` module route and an "Install from URL" installer with hash verification — desktop gets the installer too. That is a Milestone 10 step, not a spec-sized change.
3. Keep native contributions static everywhere; grow the wasm ABI (UI contributions) before promising "small app + extensions from GitHub", because until then only commands and agent tools can come from outside the APK.
4. Stores: F-Droid first (no account, no policy review of the extension mechanism, and it forces the reproducible-build discipline we want anyway); Play when there is a signed AAB and a privacy text.
