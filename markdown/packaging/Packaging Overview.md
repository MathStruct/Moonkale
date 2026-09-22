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
| `packaging/build-release.sh` | the release build: staging tree → tarball, `.deb`, Arch package, checksums (Milestone 13) |
| `packaging/arch-bin/PKGBUILD` | `moonkale-bin`: installs the release tarball (what friends use) |
| `packaging/arch/PKGBUILD` | AUR-style recipe (`moonkale-git`, from source) |
| `.github/workflows/release.yml` | Linux + Arch + Windows + macOS + Nix on `v*` tags → GitHub Release |
| `packaging/linux/moonkale.desktop` | XDG desktop entry, shared by Arch and Nix |
| `flake.nix` | Nix package, dev shell, and `nix flake check` |
| `packages/desktop/Dioxus.toml`, `packages/mobile/Dioxus.toml` | bundle metadata |

> [!info] Status (Milestone 13, 2026-09-21)
> `packaging/build-release.sh` builds the desktop app + `moonkale-server` and produces the tarball, the `.deb` (assembled by hand) and the Arch package (`packaging/arch-bin`) in `dist/`; the tarball was run from `/tmp` (assets found, editor loads), the `.deb`'s layout and control file checked, `moonkale-bin` built with `makepkg`. The from-source `moonkale-git` PKGBUILD and `flake.nix` were updated to dx's current layout (`target/dx/moonkale/…`); the flake is verified only by CI (no nix-daemon here). `.github/workflows/release.yml` builds all of it plus Windows/macOS bundles on tags. Friend-facing instructions: [[Install]].

## The two binaries
What `moonkale` and `moonkale-server` each are, what they carry and when the second one is wanted: [[Two Binaries]]. For packagers the rule is that both look for their data next to themselves — `lib/Moonkale/assets` for the app, `lib/Moonkale/public` for the server's browser client — and that the client needs its own `dx build --platform web` (P-126).

## Cutting a release (Prompt25)
The workflow `.github/workflows/release.yml` runs on any tag `v*` and attaches every package it managed to build to a GitHub Release ([[Install]]). The recipe:

```sh
git checkout master && git pull            # release from master, working tree clean
grep '^version' Cargo.toml                 # the tag must match: 0.1.0 → v0.1.0
git tag -a v0.1.0 -m "Moonkale 0.1.0"      # annotated: it carries a date and a message
git push origin v0.1.0                     # push the one tag, not --tags (that pushes every local tag)
```

Then watch *Actions → Release packages*. Since 2026-09-22 the release job runs **even when a platform job fails** (`if: always()`): the Windows, macOS and Android jobs have never run on GitHub, and a failure there must not hold back the Linux packages. The release notes list the jobs that failed, so the missing files are explained. If the Linux job itself fails, the release is created empty except for `sha256sums.txt` — delete it (*Releases → Delete*), fix, `git tag -d v0.1.0 && git push origin :v0.1.0`, and tag again; a tag must never be moved silently once someone may have downloaded from it.

### What the first `v0.1.0` run taught (2026-09-22)
Linux, Arch and Nix built; Windows, macOS and Android failed, and because the release job required *all* of them, **no release was created at all** (P-120). Fixed, with three job bugs: `--package-types msi nsis` is two flags, not one (P-121); `macos-13` was retired and its jobs queue forever, `macos-15-intel` replaces it (P-122); the Android job never installed system libraries although `dx build --platform android` also builds a host-side server binary that links WebKitGTK, GTK 3, libxdo and OpenSSL (P-123) — and dx hid cargo's error, so `build-android.sh` forwards `$DX_BUILD_ARGS` and CI passes `--verbose` (not `-v`, which dx does not have: P-124). Retagging is safe **only** while nothing was published: no release existed, so `git tag -d v0.1.0 && git push origin :v0.1.0` and tagging again is the clean move. Once a release carries files, use the next version instead.

### The third run: everything built except Windows and Android, and still no release
`macos-15-intel` fixed the queueing (both `.dmg`s built), the container image published, Linux, Arch and Nix were green — and the release job still produced nothing, because its checksum step ran `sha256sum *` over a directory that `merge-multiple` had preserved (P-127). It now flattens the artefacts into `release/` first and only refuses when nothing at all was built.

### Windows needs OpenSSL vendored
The second tagged run got past the flag bug and compiled all 1081 crates on Windows, then failed at the link step: `LNK1181: cannot open input file 'libssl.lib'` (P-125). `dioxus-desktop` pulls `tungstenite` with `native-tls`, so the desktop binary wants OpenSSL and Windows has no system copy. The desktop crate now carries `openssl-sys` with `vendored` for `cfg(windows)` only, which compiles OpenSSL from source with the perl and nasm already on the runner; Linux and macOS are untouched.

### The "Packages" box in the sidebar
GitHub's **Packages** panel is the package *registries* (ghcr.io containers, npm, Maven…) — it has nothing to do with release assets, which appear under **Releases**. "No packages published" simply means nothing was pushed to a registry. The `container` job fills it: it takes `bin/moonkale-server` out of the Linux tarball, wraps it in an Ubuntu 24.04 image (`packaging/Dockerfile`, no compiling) and pushes `ghcr.io/mathstruct/moonkale-server:<version>` and `:latest` on a tag; a manual run builds the image and stops. The workflow needs `permissions: packages: write`, which it now has. The first push creates the package as **private** — open *Packages → moonkale-server → Package settings → Change visibility → Public* once, and link it to the repository there, or friends get a 404. The image is genuinely useful beyond the sidebar: `docker run --rm -p 8080:8080 -v /srv/notes:/data -e MOONKALE_TOKEN=… -e MOONKALE_INSECURE_HTTP=1 ghcr.io/mathstruct/moonkale-server` is the shortest path to a self-hosted server ([[Remote and Server Modes]]).

Before the first tag: bump `version` in `Cargo.toml` when the last tag had the same number; `packaging/arch/PKGBUILD` and the release jobs read it from there. `dist/` is git-ignored — the local `packaging/build-release.sh` output is for hand-outs over the LAN, the release is what friends should install from.
