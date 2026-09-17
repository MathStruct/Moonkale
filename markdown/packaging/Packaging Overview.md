---
title: "Packaging Overview"
description: How a Dioxus app becomes a package — what dx produces, what the binary needs at runtime, and the three targets Moonkale ships to.
tags: [packaging]
---
Target distributions now: **Arch Linux**, **NixOS**, **Android**. Later (no test device yet): Windows (`dx bundle --package-types msi|nsis`), macOS (`dmg`, `.app` with `bundle.macos` signing), iOS (`ipa`, needs Xcode) — all produced by the same `dx bundle` and the same `Dioxus.toml`; nothing here should block them. One note each: [[Arch Linux]], [[NixOS]], [[Android]]. This note is what they all rely on. Facts were checked against `dioxus-cli` 0.7.10's source (`src/build/request.rs`, `src/config/*.rs`) and `dioxus-asset-resolver` 0.7.10 on 2026-09-17.

## What `dx build` produces

`cargo build` alone is **not enough** for a Dioxus app: every `asset!("/assets/x.css")` compiles to a *hashed* file name, and it is `dx` that reads the asset table out of the binary and copies the files next to it. So packages must call `dx build` (or `dx bundle`), never bare `cargo build`.

```sh
cd packages/desktop
dx build --release --platform linux --package desktop
# → target/dx/desktop/release/linux/app/
#     moonkale         the binary (name from [[bin]] in Cargo.toml)
#     assets/          codemirror-<hash>.js, shell-<hash>.css, …
```

`--platform linux` is dx's name for the desktop webview target on Linux. `dx bundle --package-types deb|rpm|appimage` wraps the same output with `tauri-bundler`; Arch and Nix don't need that — they lay the files out themselves.

## Where the binary looks for its assets at runtime

From `dioxus-asset-resolver::native::get_asset_root()` on Linux:

```text
<prefix>/bin/moonkale              ← the executable
<prefix>/lib/Moonkale/assets/…     ← first choice: lib/<DIOXUS_PRODUCT_NAME>/assets
<prefix>/lib/*/assets/             ← fallback: first lib/ subdir that has an assets/ dir (!)
<exe dir>/assets/                  ← last resort (what `dx build` produces in-place)
```

`DIOXUS_PRODUCT_NAME` is baked in at compile time by dx and is the **PascalCase of the binary name**. That is why the desktop crate now declares `[[bin]] name = "moonkale"` — the product name is `Moonkale`, the package installs `/usr/bin/moonkale` and `/usr/lib/Moonkale/assets/`, and the first lookup hits. Never rely on the fallback scan: in `/usr/lib` it would pick up whatever unrelated package happens to have an `assets/` directory.

## Runtime dependencies (Linux desktop)

Read straight off the link line (`-l…` flags): `webkit2gtk-4.1`, `javascriptcoregtk-4.1`, `gtk-3`, `gdk-3`, `soup-3.0`, `gio/gobject/glib-2.0`, `pango`, `cairo`, `gdk_pixbuf`, `atk`, `harfbuzz`, `xdo` (**libxdo**, via `muda` — P-038), `ssl`/`crypto`, `z`. Package-manager names are in each distro note.

## Bundle metadata: `Dioxus.toml`

`packages/desktop/Dioxus.toml` and `packages/mobile/Dioxus.toml` carry `[application] name`, `[bundle] identifier/publisher/description`, `[bundle.deb] depends`, and `[android] min_sdk/target_sdk` (+ the commented `[android.signing]` block). Keys are the ones `dioxus-cli` 0.7.10 actually parses. An icon is still missing (`bundle.icon`) — add a 512×512 PNG before the first `dx bundle`.

## Server (`api`) is a separate artefact

The web build is `dx build --release --platform web --package web` → a `server` binary + `public/` (wasm, JS, assets). It is a service, not a desktop app; package it as a systemd unit / NixOS module later. Not covered by the three notes yet.

## Files in the repo

| file | purpose |
|---|---|
| `packaging/arch/PKGBUILD` | AUR-style recipe (`moonkale-git`) |
| `packaging/linux/moonkale.desktop` | XDG desktop entry, shared by Arch and Nix |
| `flake.nix` | Nix package, dev shell, and `nix flake check` |
| `packages/desktop/Dioxus.toml`, `packages/mobile/Dioxus.toml` | bundle metadata |

> [!warning] Status
> All three recipes are written from the CLI source and the documented layout, but **none has been run on a clean machine yet**: the dev box can't link the desktop app (P-038, fixed by installing `xdotool`) and has no Nix or Android SDK. Each note ends with the exact command that proves it works; run it and log the result in [[Problem Log]].
