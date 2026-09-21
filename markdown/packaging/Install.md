---
title: "Install"
description: How to install Moonkale — Arch (pacman), Debian/Ubuntu (apt), Nix, any Linux (tarball), Windows and macOS (unsigned builds) — and what to expect.
tags: [packaging, install]
---
Releases live at <https://github.com/MathStruct/Moonkale/releases>. Each one carries the same files, built by [[Milestone 13 - Packaging|the release workflow]]: a Linux tarball, a `.deb`, an Arch package, an AppImage, Windows `.msi`/`.exe`, macOS `.dmg`, and `sha256sums.txt`. Every package contains the desktop app **and** `moonkale-server` (for *Open Remote Folder…* and self-hosting).

## Arch Linux (pacman)
```sh
sudo pacman -U moonkale-bin-<version>-1-x86_64.pkg.tar.zst
moonkale
```
Runtime packages pacman pulls in: `webkit2gtk-4.1 gtk3 libayatana-appindicator xdotool openssl`. To build from source instead: `git clone https://github.com/MathStruct/Moonkale && cd Moonkale/packaging/arch && makepkg -si` (needs `dioxus-cli` ≥ 0.7.10 from the AUR; ~10 minutes).

## Debian / Ubuntu (apt)
```sh
sudo apt install ./moonkale_<version>_amd64.deb
moonkale
```
Needs Debian 12+ or Ubuntu 22.04+ (`libwebkit2gtk-4.1-0`); apt installs `libgtk-3-0 libayatana-appindicator3-1 libxdo3` with it.

## Nix / NixOS
```sh
nix profile install github:MathStruct/Moonkale      # builds from source (10–15 min the first time)
# or try without installing:
nix run github:MathStruct/Moonkale
```
Needs `experimental-features = nix-command flakes`. The flake also has a dev shell (`nix develop`) and `nix flake check`.

## Any Linux (tarball)
```sh
tar xzf moonkale-<version>-linux-x86_64.tar.gz
cd moonkale-<version>-linux-x86_64
./bin/moonkale                    # runs from here, nothing installed
./install.sh ~/.local             # or: sudo ./install.sh /usr/local
```
Needs WebKitGTK 4.1, GTK 3 and `libxdo` from your distribution. The AppImage (`Moonkale_<version>_amd64.AppImage`, `chmod +x` then run) bundles nothing extra either — WebKitGTK is too large and too system-bound to embed.

## Windows and macOS — unsigned
GitHub's runners build `Moonkale_<version>_x64-setup.exe` / `.msi` and `Moonkale_<version>_{arm64,x86_64}.dmg` on every release, but **nobody in the project has a Windows or Mac machine to test them on**, and they are **not code-signed**:
- Windows: SmartScreen says "Windows protected your PC" → *More info* → *Run anyway*.
- macOS: Gatekeeper refuses a double-click → right-click the app → *Open* (once), or `xattr -d com.apple.quarantine /Applications/Moonkale.app`.

Signing needs a certificate (Windows, ~€200/yr) or an Apple Developer account (macOS notarisation, $99/yr); neither exists yet. If you try one of these builds, please say what happened in an issue — that is the only testing they get.

## After installing
- Start it, *File → Open Folder…*, and open a `.md` or `.rs` file. If the editor shows "Loading editor…" forever, the assets were not found next to the binary: tell us the install method.
- NVIDIA + Wayland on Linux and the window stays blank: start with `WEBKIT_DISABLE_DMABUF_RENDERER=1 moonkale` (see [[Linux Desktop Setup]]).
- Language servers (rust-analyzer, pyright, …) are found on `PATH`; Claude Code needs the `claude` CLI logged in; nothing else is required.

## Verify a download
```sh
sha256sum -c sha256sums.txt --ignore-missing
```

## What is in a package
`bin/moonkale`, `bin/moonkale-server`, `lib/Moonkale/assets/` (the editors' JavaScript bundles, KaTeX, the graph renderer), a desktop entry and icon, `share/doc/moonkale/{README.md,LICENSE,THIRD-PARTY.md}`. Moonkale is MIT; the third-party notices are in `THIRD-PARTY.md` ([[Licensing]]). Sizes: ~115 MB compressed, ~430 MB installed (the two binaries carry DuckDB, wasmtime, Typst and tree-sitter).
