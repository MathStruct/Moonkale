---
title: "Arch Linux"
description: Packaging Moonkale's desktop app for Arch (PKGBUILD / AUR).
tags: [packaging, arch]
---
Recipe: `packaging/arch/PKGBUILD`. Background: [[Packaging Overview]]. System prerequisites for *running* it: [[Linux Desktop Setup]].

## Dependencies

| kind | packages | why |
|---|---|---|
| runtime `depends` | `webkit2gtk-4.1 gtk3 libappindicator-gtk3 xdotool openssl gcc-libs glibc` | what the binary links; `xdotool` provides `libxdo.so` (P-038) |
| `makedepends` | `git cargo rust dioxus-cli pkgconf` | `dioxus-cli` is in the AUR (`dioxus-cli`, or `dioxus-cli-bin` for a prebuilt); must be **≥ 0.7.10** |

## How the PKGBUILD works

1. `pkgver()` — `0.1.0.r<commits>.g<sha>` from git (it's a `-git` package; a tagged release becomes a plain `moonkale` package with a tarball source and real checksums).
2. `prepare()` — `cargo fetch --locked` so `build()` needs no network (makepkg runs builds offline in a clean chroot with `extra-x86_64-build`).
3. `build()` — `dx build --release --platform linux --package desktop --cargo-args "--frozen"` from `packages/desktop`. **dx, not cargo**, because only dx collects the hashed assets. `CARGO_TARGET_DIR` is pinned so `package()` knows where the output is.
4. `package()` — install to the layout the asset resolver expects:
   ```text
   /usr/bin/moonkale
   /usr/lib/Moonkale/assets/…
   /usr/share/applications/moonkale.desktop
   /usr/share/icons/hicolor/512x512/apps/moonkale.png   (once an icon exists)
   ```

## Build and install

```sh
cd packaging/arch
makepkg -si            # builds in-place and installs
# or, the proper way, in a clean chroot:
# extra-x86_64-build
namcap PKGBUILD moonkale-git-*.pkg.tar.zst   # lint
```

The proof that packaging is right (not just the build): after `pacman -U`, run `moonkale` from a **different directory** than the source tree and open a file — if the editor shows "Loading editor…" forever, the CodeMirror asset was not found → the `lib/Moonkale/assets` layout or the product name is wrong.

## Publishing to the AUR

```sh
git clone ssh://aur@aur.archlinux.org/moonkale-git.git aur
cp packaging/arch/PKGBUILD aur/ && cd aur
makepkg --printsrcinfo > .SRCINFO
git add PKGBUILD .SRCINFO && git commit -m "Initial import" && git push
```

## Known gaps
- No icon yet (`bundle.icon` in `Dioxus.toml`, `hicolor` install line commented out).
- `dioxus-cli` in the AUR may lag the workspace's `dioxus` version; if so, `cargo install dioxus-cli --version 0.7.10 --locked` in `prepare()` is *not* allowed by AUR rules — pin the AUR package version in `makedepends` instead (`'dioxus-cli>=0.7.10'`).
- Reproducibility: dx's asset hashes are content-based, so two builds of the same commit produce the same file names — good for `pacman` diffs.
