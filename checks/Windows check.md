# Windows check (first Windows run ever)

Written by Claude for Daniel, 2026-10-17. Nothing in Moonkale has been built or tested on Windows except the release installers (`release.yml`, on a tag). CI has **no** Windows test job. The new Windows file-rename code from issue #15 (`MoveFileExW` without replace) has never been compiled. Whatever fails here is the useful result.

## 0. Setup (a separate Windows user account if possible)

1. **Visual Studio Build Tools** → workload "Desktop development with C++" (MSVC linker; the bundled C++ stores need it).
2. **Rust**: rustup from https://rustup.rs, default `x86_64-pc-windows-msvc`.
3. **Git for Windows**, then:
   ```powershell
   git config --global core.longpaths true
   git config --global core.autocrlf false
   ```
4. **dioxus-cli 0.7.10** (what CI uses): `cargo install cargo-binstall`, then `cargo binstall -y dioxus-cli@0.7.10`.
5. WebView2: built into Windows 11; on Windows 10 install the "Evergreen" runtime.
6. Clone somewhere short (long paths): `git clone https://github.com/MathStruct/Moonkale.git C:\mk` and `cd C:\mk`.
7. Optional, Claude on that PC: `irm https://claude.ai/install.ps1 | iex`, then `claude`. Turn on `/remote-control` so the Linux session can try to reach it. **Afterwards: `/logout`**, sign out of `gh`, and remove the account or profile.

Never copy `.secrets/` there; nothing below needs it. Disk: 20–40 GB for `target`. First build: 30–60 min.

## 1. The folder source (the most valuable 15 minutes)

```powershell
cargo test -p moonkale-project-fs -p moonkale-index 2>&1 | Tee-Object fs.txt
```

This checks the issue #15 code on Windows: non-replacing renames, the locked save path, content versions, and the `races.rs` tests. Keep `fs.txt` whatever happens.

## 2. The whole workspace

```powershell
cargo test --workspace 2>&1 | Tee-Object ws.txt
```

Expect some failures (paths with `\`, CRLF, file locking, Linux-only stores like LadybugDB). Keep `ws.txt`.

## 3. The desktop app

```powershell
cd packages\desktop
dx serve --platform desktop --features desktop
```

Check and screenshot (Win+Shift+S):
- open a folder (the clone itself), the tree, a `.rs` file in the Rust editor, an edit, Ctrl+S, the file on disk;
- a Markdown file in Rich and Source view;
- the graph view (WebView2 may give real WebGPU here, unlike Linux);
- the shortcuts: Ctrl+P, Ctrl+Shift+P, Ctrl+F;
- window controls, a resize, and closing and reopening (is the layout restored?).

## 4. If there's time: the installer

```powershell
cd packages\desktop
dx bundle --platform desktop --release --features desktop --package-types msi --package-types nsis
```

Install from `target\dx\moonkale\bundle\`, start it from the Start menu, and uninstall it.

## Getting results back

Any one of these works:
- Claude on the Windows PC commits `fs.txt`, `ws.txt` and notes to a branch `windows-check` and pushes it (needs `gh auth login` there; log out afterwards);
- or paste the failing parts into the Linux session over ssh/tmux;
- or photos of the screen.
