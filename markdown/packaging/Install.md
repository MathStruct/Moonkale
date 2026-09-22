---
title: "Install"
description: How to install Moonkale — Arch (pacman), Debian/Ubuntu (apt), Nix, any Linux (tarball), Windows and macOS (unsigned builds) — and what to expect.
tags: [packaging, install]
---
**Downloads: <https://github.com/MathStruct/Moonkale/releases/latest>** — the *Releases* box in the right sidebar of the repository page leads there too, and every file is listed under *Assets* at the bottom of the release (GitHub collapses that list, which is why it is easy to miss). Each one carries the same files, built by [[Milestone 13 - Packaging|the release workflow]]: a Linux tarball, a `.deb`, an Arch package, an AppImage, an Android APK, Windows `.msi`/`.exe`, macOS `.dmg`, and `sha256sums.txt`. Every package contains the desktop app **and** `moonkale-server` (for *Open Remote Folder…* and self-hosting).

## Arch Linux (pacman)
```sh
sudo pacman -U moonkale-<version>-1-x86_64.pkg.tar.zst
moonkale
```
Runtime packages pacman pulls in: `webkit2gtk-4.1 gtk3 libayatana-appindicator xdotool openssl`.

> [!warning] v0.1.0's Arch package is broken — use a source build
> It was the Ubuntu tarball repackaged, and Arch's xdotool 4 has no `libxdo.so.3`, so the app does not start (P-129). From the next release the Arch package is built from source on Arch. For 0.1.0: `makepkg -si` below, or wait for the next tag. To build from source instead: `git clone https://github.com/MathStruct/Moonkale && cd Moonkale/packaging/arch && makepkg -si` (needs `dioxus-cli` ≥ 0.7.10 from the AUR; ~10 minutes).

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
Needs `experimental-features = nix-command flakes`. **Add the binary cache first** or the build takes over an hour:
```sh
nix profile install nixpkgs#cachix && cachix use moonkale
```
(the cache is public and free — [[NixOS]] explains it; it is populated by the release workflow). On **NixOS** that is all. On another distro with Nix installed, a Nix-built GTK app cannot open the GPU on its own (`Could not create default EGL display`): run it through [nixGL](https://github.com/nix-community/nixGL) — `nix run --impure github:nix-community/nixGL -- moonkale` — or use the tarball/package for your distro instead. The flake also has a dev shell (`nix develop`) and `nix flake check`.

## Any Linux (tarball)
```sh
tar xzf moonkale-<version>-linux-x86_64.tar.gz
cd moonkale-<version>-linux-x86_64
./bin/moonkale                    # runs from here, nothing installed
./install.sh ~/.local             # or: sudo ./install.sh /usr/local
```
Needs WebKitGTK 4.1, GTK 3 and `libxdo` **in the versions Ubuntu 24.04 has**, because that is where the binaries are built: `libxdo.so.3` specifically (Arch's xdotool 4 provides `libxdo.so.4` and the app will not start — P-129). On a non-Debian distribution, use that distribution's package or build from source. The AppImage (`moonkale_<version>_x86_64.AppImage`, `chmod +x` then run) carries a few libraries but not WebKitGTK, and it looks for WebKit's helper processes under Debian's path (`/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1/`), so it too is effectively a Debian/Ubuntu artefact today.

## Android
`moonkale-<version>-android-arm64.apk` (release build, debug-signed, ~45 MB; Android 8+ on arm64). Allow "install from unknown sources" for your file manager, or from a computer with `adb`: `adb install -r moonkale-<version>-android-arm64.apk`. The app opens its own vault in its private storage; **File → Connect to Server…** makes it a client of a Moonkale server (the folder, terminal, git and agent sessions run there). Built by `packages/mobile/build-android.sh` (needs the Android SDK + NDK, see `packages/mobile/README.md`) and by the release workflow.

> [!note] Which Windows file?
> `Moonkale_<version>_x64-setup.exe` is the NSIS installer and `Moonkale_<version>_x64.msi` the MSI — take either. The v0.1.0 release also carries a bare `moonkale.exe`, which is the unpackaged binary that slipped into the upload (P-128); it works but installs nothing, and it is gone from later releases.

## Windows and macOS — unsigned
GitHub's runners build `Moonkale_<version>_x64-setup.exe` / `.msi` and `Moonkale_<version>_{arm64,x86_64}.dmg` on every release, but **nobody in the project has a Windows or Mac machine to test them on**, and they are **not code-signed**:
- Windows: SmartScreen says "Windows protected your PC" → *More info* → *Run anyway*.
- macOS: Gatekeeper refuses a double-click → right-click the app → *Open* (once), or `xattr -d com.apple.quarantine /Applications/Moonkale.app`. Shortcuts are Cmd-based on a Mac (`Cmd+O`, `Cmd+P`, `Cmd+Shift+P`); Ctrl+letter is left to the text system as macOS users expect ([[027]]).

Signing needs a certificate (Windows, ~€200/yr) or an Apple Developer account (macOS notarisation, $99/yr); neither exists yet. If you try one of these builds, please say what happened in an issue — that is the only testing they get ([[Feedback]]).

## After installing
- Start it, *File → Open Folder…*, and open a `.md` or `.rs` file. If the editor shows "Loading editor…" forever, the assets were not found next to the binary: tell us the install method.
- NVIDIA + Wayland on Linux and the window stays blank: start with `WEBKIT_DISABLE_DMABUF_RENDERER=1 moonkale` (see [[Linux Desktop Setup]]).
- Language servers (rust-analyzer, pyright, …) are found on `PATH`; Claude Code needs the `claude` CLI logged in; nothing else is required.

## Server in a container
```sh
docker run --rm -p 8080:8080 -v /srv/notes:/data \
  -e MOONKALE_TOKEN=$(openssl rand -hex 16) -e MOONKALE_INSECURE_HTTP=1 \
  ghcr.io/mathstruct/moonkale-server
```
`ghcr.io/mathstruct/moonkale-server:<version>` / `:latest` holds `moonkale-server` and its browser client (Ubuntu 24.04, ~190 MB), serving `/data`. It is the server only — the desktop app is not in it ([[Two Binaries]]). A token is required for any non-loopback bind; put TLS or a reverse proxy in front and drop `MOONKALE_INSECURE_HTTP` ([[Remote and Server Modes]]). The desktop app then connects with *File → Connect to Server…*.

## Verify a download
```sh
sha256sum -c sha256sums.txt --ignore-missing
```

## What is in a package
`bin/moonkale` (the desktop app), `bin/moonkale-server` (the server — [[Two Binaries]] explains the difference), `lib/Moonkale/assets/` (the editors' JavaScript bundles, KaTeX, the graph renderer), `lib/Moonkale/public/` (the server's browser client), a desktop entry and icon, `share/doc/moonkale/{README.md,LICENSE,THIRD-PARTY.md}`. Moonkale is MIT; the third-party notices are in `THIRD-PARTY.md` ([[Licensing]]). Sizes: ~130 MB compressed, ~420 MB installed (the two binaries carry DuckDB, wasmtime, Typst and tree-sitter).
