---
title: "Milestone 13 — Implementation Log"
description: What was built for "Packaging" — one release script for the Linux artefacts (tarball, .deb, Arch package), fixed from-source recipes (PKGBUILD, flake), a GitHub release workflow with Windows/macOS bundles, the Install page, and extension settings shown with the extension.
tags: [milestone, log, packaging, release]
---
Plan: [[Milestone 13 - Packaging]] (from [[Prompt21]]).

> [!success] Steps 1–4 done (2026-09-21) — three Linux packages built and checked here; Nix, Windows and macOS left to the workflow
> `packaging/build-release.sh` builds the desktop app and `moonkale-server` (release) and stages one tree, from which it makes **`moonkale-0.1.0-linux-x86_64.tar.gz`** (with an `install.sh`), **`moonkale_0.1.0_amd64.deb`** (assembled with `ar`+`tar`, no Debian tooling) and **`moonkale-bin-0.1.0-1-x86_64.pkg.tar.zst`** (`packaging/arch-bin`, via `makepkg`) plus `sha256sums.txt` — 115 MB compressed each, 291 MB unpacked. The tarball was unpacked under `/tmp` and the app started from there: assets found, a `.md` opened with highlighting (the proof the layout is right), `moonkale-server` next to the binary for remote mode. The from-source **`moonkale-git` PKGBUILD** and **`flake.nix`** were brought to dx's current layout. **`.github/workflows/release.yml`** builds Linux + Arch + Windows (`.msi`, NSIS) + macOS (`.dmg` ×2) + a Nix build on `v*` tags and attaches everything to a GitHub Release. **[[Install]]** is the page to send friends. **Extension settings** now render under the extension's row (`Extension::settings`).

## Steps as executed

| # | step | outcome | notes |
|---|---|---|---|
| 1 | `packaging/build-release.sh` (`--no-build`, `--no-arch`), staging tree (`bin/{moonkale,moonkale-server}`, `lib/Moonkale/assets`, desktop entry, icon, `share/doc/moonkale/{README,LICENSE,THIRD-PARTY}`), `install.sh`, tarball, `.deb` (control, `postinst`, `md5sums`, `Depends` from `Dioxus.toml`), `packaging/arch-bin/PKGBUILD` (`options=('!debug' '!strip')`, `MOONKALE_VERSION`), fixes to `packaging/arch/PKGBUILD` (paths, `--platform desktop`, server binary, `libayatana-appindicator`, MIT, `MOONKALE_GIT_URL` for local tests) and `flake.nix` (paths, server, icon, `licenses.mit`) | ✅ script run on the existing release builds: three artefacts; tarball run from `/tmp` (screenshot-checked: editor loads); deb inspected (`ar t`, control); `makepkg -f` of `moonkale-bin`; `moonkale-git` built by `makepkg` from the local checkout (see Numbers) | [[Packaging Overview]] |
| 2 | `.github/workflows/release.yml`: jobs `linux` (apt deps, `cargo-binstall dioxus-cli@0.7.10`, the script, AppImage via `dx bundle`), `arch` (`archlinux` container, `makepkg` as an unprivileged user on the Linux artefact), `windows` (`dx bundle --package-types msi nsis`), `macos` (matrix `macos-14` arm64 / `macos-13` x86_64, `dmg`), `nix` (`cachix/install-nix-action`, `nix build`), `release` (checksums, `softprops/action-gh-release`) | ⏳ YAML valid; runs on the next tag or *Run workflow* — cannot run here | the Linux job is the local script, so its steps are the ones verified |
| 3 | `Extension::settings(&self, ws, SettingsTarget) -> Option<Element>` + `Workspace::update_settings_in`; sections for code (wrap), markdown (Rich), terminal and terminal-native (shell, implementation), agent (on the server); Settings drops Editor/Terminal and the on-server checkbox | ✅ `extensions.mjs` step: the Terminal row's select persists `terminal.implementation`; the Markdown row has its checkbox; `settings`, `highlight`, `rich` suites still pass | [ext-api.md](https://github.com/MathStruct/Moonkale/blob/master/packages/ext-api/ext-api.md), [ui.md](https://github.com/MathStruct/Moonkale/blob/master/packages/ui/ui.md) |
| 4 | [[Install]], README install table, [[Packaging Overview]] status, [[Arch Linux]], [[NixOS]], [[Extension Catalogue]] note, this log | ✅ | |

## What GitHub can do for Windows and macOS (the question)
Everything except signing. GitHub-hosted runners for public repositories are free for `windows-latest` and `macos-13`/`macos-14` (Intel and Apple silicon); `dx bundle` on them produces `.msi`/NSIS `.exe` and `.dmg`, and the workflow attaches them to the release. What they cannot do: **code signing** — Windows needs a code-signing certificate (SmartScreen otherwise warns "unknown publisher"), macOS needs an Apple Developer account for notarisation (Gatekeeper otherwise wants right-click → Open). Both cost money and are Daniel's decision; until then the builds are labelled unsigned and untested on the [[Install]] page, and whoever tries one is asked to report. Testing them here is impossible (no machine); a friend with either OS is the test.

## What to switch on here (Daniel)
- `sudo systemctl enable --now nix-daemon` and `experimental-features = nix-command flakes` in `/etc/nix/nix.conf` (Arch's `nix` package has no `nix-users` group — its daemon socket is world-writable; a shell needs `NIX_REMOTE=daemon`, which `/etc/profile.d/nix-daemon.sh` sets on login) → `nix build` can be verified locally (the flake is otherwise verified only by the workflow).
- `sudo systemctl enable --now docker` + `docker` group → the `.deb` can be installed in a `debian:12` container as a check (`apt install ./moonkale_0.1.0_amd64.deb`).
- The first release: `git tag v0.1.0 && git push --tags` runs the workflow; the AUR upload of `moonkale-git`/`moonkale-bin` needs an AUR account.

## Nix, verified later the same day
Daniel switched the daemon on (Arch's `nix` package: no `nix-users` group; the socket is world-writable, `NIX_REMOTE=daemon` from `/etc/profile.d/nix-daemon.sh`). `nix build .#default` then failed twice and succeeded on the third try: `--cargo-args=--frozen` (dx 0.7.10 rejects the two-token form), and LadybugDB's build-time download (P-115) — the flake now fetches the prebuilt `liblbug` archive as a fixed-output derivation. Result: `moonkale` + `moonkale-server` + assets in ~15 min. Running it on this Arch host needs `nixGL` (EGL); on NixOS it does not. [[NixOS]] has the details.

## Android APK (Prompt23, 2026-09-22)
`packages/mobile/build-android.sh` → `dist/moonkale-0.1.0-android-arm64.apk`, **45 MB** (23 MB in Milestone 9: the phone now carries `api` + the client relay with rustls, the Rust terminal's `vt100` and the Rust code editor's 20 tree-sitter grammars). Built here in 3 minutes; not installed — the phone was not connected (`adb install -r dist/moonkale-0.1.0-android-arm64.apk` when it is). An `android` job (`setup-android` with NDK 29, JDK 17, the same script) joined `release.yml`; it is untested there like the other non-Linux jobs.

## Deviations from the plan
1. **`THIRD-PARTY.md` lists the crates only when `cargo-license` is installed** (it is not here); the file always names the JavaScript bundles and fonts and links the [[Licensing]] page. `cargo install cargo-license` once makes the full list appear in the next build.
2. **The AppImage is built only in CI** (`dx bundle --package-types appimage` downloads `linuxdeploy` at build time; not tried here) and is marked non-fatal in the workflow.
3. **No RPM**, no Flatpak: nothing to test them on.
4. **`moonkale-bin`'s `sha256sums` is `SKIP`** for local builds; the release workflow builds it from the artefact with `--skipchecksums`. A published AUR `-bin` package needs the real sum per release.

## Problems hit (→ [[Problem Log]])
- **P-111** `makepkg`'s default `lto` option turns the C++ objects of DuckDB and ring into GCC bitcode `rust-lld` cannot link → `options=('!debug' '!lto')`.

## Numbers
- Artefacts: tarball 115.2 MB, `.deb` 115.2 MB, Arch package 107.6 MB (zstd); installed 291 MB (`moonkale` 147 MB + `moonkale-server` 142 MB stripped + 7 MB assets).
- `build-release.sh --no-build` (packaging only): ~40 s, dominated by compression.
- `moonkale-git` via `MOONKALE_GIT_URL=file://… makepkg -f` from the local checkout: fetch + full release build of app and server in **~8 minutes**, `moonkale-git-0.1.0.r55.g1eda12a-1-x86_64.pkg.tar.zst` 105.7 MB — after P-111 (`!lto`), the first attempt failed at the link with undefined DuckDB/ring symbols.
