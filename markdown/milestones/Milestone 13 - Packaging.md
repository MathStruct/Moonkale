---
title: "Milestone 13 — Packaging: the plan"
description: Packages friends can install — pacman, apt, nix — with one-line instructions; a release pipeline on GitHub Actions that also produces Windows and macOS bundles nobody here can test; and every extension's settings shown with the extension. Record in Milestone 13 - Implementation Log.
tags: [milestone, planning, packaging, release]
---
From [[Prompt21]] (2026-09-21): *I would like to give this project to my friends* — a package for **pacman**, **apt** and **nix**, something to send with a short install instruction; *to what degree can GitHub build Windows and macOS packages* (untestable here); and *extension settings should be appended to the extension*.

Record: [[Milestone 13 - Implementation Log]]. Builds on [[Packaging Overview]], [[Arch Linux]], [[NixOS]], [[Licensing]] (a package is a combination — the third-party notices travel with it).

## Starting point
- `packaging/arch/PKGBUILD` (`moonkale-git`, from source), `flake.nix`, `packaging/linux/moonkale.desktop`, `Dioxus.toml` bundle metadata — written in Milestone 6 against dx's *old* output layout (`target/dx/desktop/...`; the app is `Moonkale` since Milestone 10, so it is `target/dx/moonkale/release/linux/app`), never run on a clean machine, and with the pre-MIT licence string.
- `dx bundle --platform desktop --release --package-types deb` **works today** (tried: `moonkale_0.1.0_amd64.deb`, 68 MB compressed, `usr/bin/moonkale` + `usr/lib/Moonkale/assets` + icon + desktop entry, `Maintainer: Unknown`, no server binary).
- Remote mode (Milestone 11) wants `moonkale-server` next to the executable to upload to hosts; Milestone 12's Claude Code needs nothing packaged (the CLI is the user's).
- Tools here: `makepkg` yes; `nix` installed but the daemon is not running (`opening lock file … Permission denied`); `docker` installed, daemon not running. No Windows or macOS anywhere. Releases: none tagged yet; the repo is public.

## Scope: what "done" means
1. **One staging tree for every Linux package** — `packaging/build-release.sh` builds the desktop app and the server (release), and stages `dist/moonkale-<version>-linux-<arch>/` with `bin/moonkale`, `bin/moonkale-server`, `lib/Moonkale/assets/`, `share/applications/moonkale.desktop`, `share/icons/hicolor/512x512/apps/moonkale.png`, `share/doc/moonkale/{README.md,LICENSE,THIRD-PARTY.md}`; from it: a **tarball** (works on any distro: unpack to `/opt` or `~/.local`), a **`.deb`** (built with plain `ar`+`tar`, no Debian tooling needed on the build machine; `Depends` from `Dioxus.toml`; a `postinst` for the icon cache), and an **Arch binary package** (`packaging/arch-bin/PKGBUILD`: `moonkale-bin`, installs the tarball — seconds, not a ten-minute build). The from-source `moonkale-git` PKGBUILD is fixed for the current layout and kept for the AUR.
2. **Nix**: `flake.nix` fixed for the current layout and the server binary, `nix build` / `nix profile install github:MathStruct/Moonkale` as the from-source path; a `nix run` and a dev shell. *Verified only if the daemon gets switched on here* (see "what to install" below); otherwise CI verifies it.
3. **A release pipeline** — `.github/workflows/release.yml` on `v*` tags (and by hand): Linux runner → tarball + deb + Arch package (`archlinux` container with `makepkg`) + AppImage (`dx bundle`); **Windows** runner → `.msi` + NSIS `.exe`; **macOS** runner → `.dmg` (x86_64 and arm64); Nix job → `nix build`; every artefact attached to the GitHub Release with `sha256sums.txt`. Unsigned: Windows shows SmartScreen's "unknown publisher", macOS Gatekeeper wants right-click → Open (notarisation needs a paid Apple developer account; a signing certificate for Windows likewise costs) — documented on the install page, not hidden.
4. **An install page** — `markdown/packaging/Install.md` (and the same text in the README): one block per platform, the three commands a friend types, what to expect (unsigned warnings, WebKitGTK on Linux), and how to report a problem.
5. **Extension settings with the extension** — `Extension::settings(&self, ws) -> Option<Element>`: a section each extension renders; the Extensions panel (and Settings → Extensions) shows it under the extension's row. Moved there: code editor (wrap), markdown (Rich by default), terminal (shell, implementation), agent (run on the server; the `claude-code` knobs stay with the provider under *Language model*), git (nothing yet). Settings → Editor/Terminal sections go, Settings keeps the cross-cutting ones (language model, policy, search, keybindings, You).
6. Verify what can be verified here (tarball run from `/tmp`, deb contents, `makepkg -si` of `moonkale-bin`, the `-git` PKGBUILD in `makepkg`'s own build), log, vault, catalogue.

**Deferred**: signing/notarisation, the AUR upload (Daniel's account), Flatpak/Snap, an RPM (`dx bundle --package-types rpm` exists, no Fedora here to test), Windows and macOS smoke tests (no machine).

## What to install / switch on here (Daniel)
- **Nix**: `sudo systemctl enable --now nix-daemon` and `experimental-features = nix-command flakes` in `/etc/nix/nix.conf` (no `nix-users` group on Arch — the socket is world-writable; `NIX_REMOTE=daemon` in the shell, set by `/etc/profile.d/nix-daemon.sh` on login) — then `nix build` can be verified locally instead of only in CI.
- **Docker** (optional, to test the `.deb` in a real Debian): `sudo systemctl enable --now docker` and `sudo usermod -aG docker daniel`.
- Nothing else: `makepkg`, `ar`, `tar`, `dx` are here.

## Architecture decisions
- **Build once, package three ways.** dx produces the app tree; everything Linux is a re-labelling of the same `dist/` directory. No packager builds the code itself except the from-source recipes (`-git` PKGBUILD, the flake), which exist for people who want to build.
- **The server binary ships in every desktop package** (`bin/moonkale-server`, ~190 MB uncompressed): it is what *Open Remote Folder…* uploads to hosts, and `moonkale-server --root … --token-stdin` is the self-hosted server. A `moonkale-server`-only package is a later split.
- **`.deb` by hand, not by tauri-bundler**: the bundler's deb has no maintainer, no server binary, no docs, and is one more tool whose layout can change; `ar` + `tar` + a control file is 40 lines and identical everywhere.
- **Binary Arch package for friends, source package for the AUR**: `-bin` installs in seconds from the release tarball; `-git` is what the AUR expects.
- **Unsigned Windows/macOS builds are still worth producing**: they let anyone with such a machine try Moonkale and report; the page says what the warning means.

## Steps

| # | step | verify |
|---|---|---|
| 1 | `packaging/build-release.sh` (desktop + server release builds, staging, tarball, deb, THIRD-PARTY notice via `cargo license` if available); `packaging/arch-bin/PKGBUILD`; fix `packaging/arch/PKGBUILD` + `flake.nix` layout | run the script; unpack the tarball under `/tmp` and start `moonkale` from there (assets found, a `.md` opens, remote upload finds `moonkale-server`); `ar t`/`dpkg-deb`-less check of the deb; `makepkg -si` of `moonkale-bin` then `pacman -Ql moonkale-bin`; `makepkg -s` of `moonkale-git` from the local checkout |
| 2 | `.github/workflows/release.yml` (matrix; `cargo-binstall dioxus-cli`; artefacts + checksums; GitHub Release) | `workflow_dispatch` on a branch once pushed — cannot run here; YAML checked, the Linux job's commands are the script from step 1 |
| 3 | `Extension::settings` + the panel section; move the editor/markdown/terminal/agent settings | E2E `extensions.mjs` extended: the Terminal row shows the implementation select, changing it persists; `settings.mjs` adjusted |
| 4 | `Install.md`, README, [[Packaging Overview]] status, catalogue, log | site rebuilt |

## Risks
| risk | mitigation |
|---|---|
| A friend's distro lacks `libwebkit2gtk-4.1` (older Ubuntu has 4.0) | the deb `Depends` names 4.1 (Ubuntu ≥ 22.04, Debian ≥ 12); the page says so; the tarball has the same requirement |
| The 190 MB server binary doubles the download | tarball ~110 MB compressed; acceptable for a first release; a server-less variant later |
| CI's dx version drifts from the workspace's dioxus | pinned `dioxus-cli@0.7.10` in the workflow and the recipes |
| Nix flake untested here | CI job; Daniel can enable the daemon |
