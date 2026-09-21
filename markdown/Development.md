---
title: Development
description: How to build, run and test Moonkale, and how this vault becomes the website.
tags: [meta, development]
---
Everything that used to be in the README about building lives here.

## Prerequisites

- Rust stable (the workspace is checked on 1.98) — `rustup` recommended.
- The Dioxus CLI: `curl -sSL http://dioxus.dev/install.sh | sh` (installs `dx`).
- For the web target: `rustup target add wasm32-unknown-unknown`.
- Linux desktop: WebKitGTK, GTK and **xdotool** (`libxdo`, linked by Dioxus's menu crate) — see [[Linux Desktop Setup]] (also covers NVIDIA/Wayland quirks).
- For the website: Node ≥ 22 (`site/.node-version`).

## Workspace layout

```
Moonkale/
├─ Cargo.toml            # workspace; every crate under packages/ is a member
├─ packages/
│  ├─ web/ desktop/ mobile/   platform entrypoints (routers, platform assets)
│  ├─ ui/                     the workbench shell (dioxus-workbench)
│  ├─ api/                    fullstack server functions
│  ├─ core/ ext-api/ ext-host/ sources*/ project-fs/ index/ llm/ lsp*/ terminal*/
│  │                          the architecture skeleton (comment-only today)
│  ├─ editors/{code,markdown,table,graph,graph-desktop,flow,terminal}/
│  └─ js/{codemirror,milkdown,xterm}/   isolated TypeScript view packages
├─ markdown/             # this vault (design record; published as the website)
├─ site/                 # vendored Quartz + TikZ/Typst/Tabs plugins
└─ .github/workflows/    # Pages deploy
```

The crate-by-crate rationale is in [[Project Structure]]; what runs on which platform in [[Platform Matrix]].

## Build and check

```sh
cargo check --workspace                 # all crates, native
cargo check -p web --target wasm32-unknown-unknown   # proves no native crate leaked into the web build
cargo clippy --workspace                # clippy.toml bans holding signal guards across .await
cargo fmt --all --check
```

## Run

Each platform crate is served with `dx` from its own directory:

```sh
cd packages/web     && dx serve                       # http://127.0.0.1:8080
cd packages/desktop && dx serve --platform desktop    # native window (needs a display)
cd packages/mobile  && dx serve --platform android    # or ios; needs the SDKs
```

`dx serve` hot-reloads `rsx!` and assets; press `r` to force a rebuild, `v` for verbose logs. Where output goes on each platform — including compiled release builds — is in [[Debugging and Logging]].

Headless smoke test of the web build (what CI will do):

```sh
(cd packages/web && dx serve --port 8080 &)
until curl -sf http://127.0.0.1:8080/ >/dev/null; do sleep 1; done
firefox --headless --profile /tmp/ffp --window-size=1400,900 --screenshot /tmp/shot.png http://127.0.0.1:8080/
```

## Test

```sh
cargo test --workspace                  # native suite (core, project-fs, …)
cargo nextest run --workspace           # parallel, per-process (cargo install cargo-nextest)
```

End-to-end (real browser against the web build): `packages/web/tests/e2e/README.md`.

## Running Milestone 1

- **Web**: `cd packages/web && MOONKALE_ROOT=/some/folder dx serve`, open <http://127.0.0.1:8080>, leave the path blank and click *Open* (or type a path under `MOONKALE_ROOT`). The folder lives on the server.
- **Desktop**: `cd packages/desktop && dx serve --platform desktop`, type any local path, *Open*. The folder is read in-process.
- Ctrl+S saves; a file changed outside the editor makes *Save* fail with a conflict banner — *Reload* takes the external version.
- Windows: **View → New Window** (`Ctrl+Shift+N`). Drag an editor **tab** onto another window to move it there; if the drop doesn't land, release anyway — the other window shows a *Move it here* banner. The status bar shows how many windows are in the session; the `dx serve` terminal logs every `session[…]` message (see [[P-045 Cross-window drag and drop]] for what to report).
- Menus: **File** (Open Folder… `Ctrl+O` — native dialog on desktop, Save, Close Editor `Ctrl+W`, Exit), **Edit** (Undo/Redo), **View** (Reset Layout, Toggle Developer Tools in debug builds), **Help** (About). On desktop the window is undecorated and the bar carries minimize/maximize/close; drag the empty bar area to move, double-click to maximize, edges to resize.
- Databases: click a `.sqlite`/`.db` file in the Explorer (or type its path in *Open*) — it appears as a source with its tables; click a table for the read-only grid + SQL box. Graph tab: Whole/Local, kind filters, hover, double-click opens the file.
- The graph renderer is a separate wasm module: rebuild with `packages/editors/graph-render/build.sh` after changing `packages/editors/graph-render/src` (needs `wasm32-unknown-unknown` and dx's `wasm-bindgen` 0.2.128 under `~/.local/share/.dx/tools`); the output in `packages/editors/graph/assets/` is committed.
- **Milestone 3 tools:** *View → New Terminal* (Ctrl+`) opens a shell in the bottom tile — on web it runs **on the server** (dev only, no auth; cwd jailed to `MOONKALE_ROOT`). Open a `.typ` file for the live Typst preview. Open a `.rs` file in a cargo project for rust-analyzer (`rustup component add rust-analyzer`; the status bar says what is missing) — on web the server spawns it. A `.lbug`/`.kuzu` database (file or directory) in the folder opens as a source (make one with `cargo run -p moonkale-sources-graph --features ladybug --example seed_people -- ~/moonkale-sample/people.lbug`); tables open the Cypher box; *Show in Graph* draws the result in the Graph tab's source picker.
- **Settings (Milestone 5):** `Ctrl+,` opens the Settings panel. User scope: `~/.config/moonkale/settings.json` (desktop) or `localStorage` (web); workspace scope: `<folder>/.moonkale/settings.json` (also the layout and open documents). API keys go into `~/.config/moonkale/secrets.json` via *Store secret* (or `MOONKALE_SECRET_<NAME>`); settings only hold the secret's name. External agents: `claude mcp add --transport http moonkale http://127.0.0.1:8080/mcp` (read-only; `MOONKALE_MCP_TOKEN` adds bearer auth).
- **Extensions (Milestone 6):** Settings → Extensions toggles built-ins and grants permissions (`extensions.enabled/disabled/permissions` in either scope). The **Flow editor** and the **Lux.jl blocks** are off by default: enable both, then File → New Flow… (`*.flow.json`), place blocks from the palette, wire typed ports, **Generate** writes `model.jl` next to the flow. **wasm extensions**: `packages/extensions/wordcount/build.sh` (needs `rustup target add wasm32-unknown-unknown`) installs the example into `~/.config/moonkale/extensions/`; modules there and in `<folder>/.moonkale/extensions/` are listed in Settings (off, no permissions) and their commands become agent tools once enabled. On web the modules run on the server (`MOONKALE_CONFIG_DIR` picks its config dir). Below 700 px the shell collapses to one tile with a bottom bar.
- **Daily driver (Milestone 7):** `Ctrl+Shift+P` command palette, `Ctrl+P` quick open (`path:line`), Settings → Keybindings to rebind. Right-click in the Explorer for New File / New Folder / Rename / Delete (to `.moonkale/trash/`); drag a file onto a folder to move it. `Ctrl+F` / `Ctrl+H` in an editor; Search → Replace with… for the workspace. With rust-analyzer: completion, `F2` rename, `Ctrl+.` code actions, `Shift+F12` references. **Changes** tab: stage/commit/discard, diffs, **Graph** for the history. **Exposing the web server**: `MOONKALE_TOKEN=<random> dx serve --addr 0.0.0.0` (a non-loopback bind without a token refuses to start); browsers log in at `/login` (HttpOnly cookie), scripts send `Authorization: Bearer <token>`; every relay call is logged under the `moonkale::audit` target. Put a reverse proxy with TLS in front and let it set `X-Forwarded-Proto` / `X-Forwarded-For`.
- **Research (Milestone 8):** the **History** tab lists the workspace's entity log (`.moonkale/history.jsonl`; Settings → You → Name is the actor); **presence** works on web (a room per folder on the server; badges in the status bar, tabs and Explorer); Graph → **3D** (right-drag or Shift-drag orbits, wheel dollies); wasm extensions run *in the browser* when the page is cross-origin isolated (the server sends COOP/COEP; `MOONKALE_ISOLATE=0` turns that off). The renderer test (`graph3d.mjs`) needs Playwright's Chromium: `npx playwright install chromium` in the E2E working dir.
- **Second halves (Milestone 9):** History → **Compact** / **Restore**; presence cursors need nothing extra; the desktop joins a hub with `MOONKALE_HUB=http://host:port` (and `MOONKALE_TOKEN` if the server has one). DuckDB is built in on desktop and the server (`moonkale-sources-sql/duckdb`, bundled; ~3 min extra on a clean build): click a `.csv`/`.tsv`/`.parquet` to get its folder as tables, or a `.duckdb` file. **Android** (verified on a Galaxy S10e): `export ANDROID_HOME=~/Android/Sdk ANDROID_NDK_HOME=$ANDROID_HOME/ndk/29.0.14206865 NDK_HOME=$ANDROID_NDK_HOME JAVA_HOME=/usr/lib/jvm/java-17-openjdk`, `rustup target add aarch64-linux-android`, then `cd packages/mobile && dx build --release --platform android --features mobile --target aarch64-linux-android` and `adb install -r target/dx/mobile/release/android/app/app/build/outputs/apk/debug/app-debug.apk` — release, because the debug APK is x86_64 and too large for a small phone. Details and the inspection recipe (screenshots, WebView DevTools, `run-as`) in `packages/mobile/README.md`.
- **Remote (Milestone 11):** **File → Open Remote Folder…** in the desktop app (or `moonkale --ssh "host:/path"`, `MOONKALE_SSH=…`): the host typed as after `ssh` — `SSH_AUTH_SOCK=0 -p 443 daniel@192.168.178.62`, `-i ~/.ssh/MathStruct daniel@dtrmblog.de`, or a `~/.ssh/config` alias (offered) — and the folder path; the `ssh` runs as a terminal tab (answer its prompts there), the server is uploaded once per version into `~/.local/share/moonkale/server/<version>/` on the host — it is whatever `MOONKALE_SERVER_BINARY` names, else `moonkale-server` next to the app, else the dev build from `cd packages/web && dx build --platform server --release` (`target/dx/web/release/web/server`, 188 MB). *Disconnect Remote* or closing the folder ends the session. **Desktop as a client** of any server: `MOONKALE_REMOTE=http://host:port MOONKALE_TOKEN=… ./moonkale`. **Standalone server**: `MOONKALE_TOKEN=… ./server --port 8443 --bind 0.0.0.0 --root /srv/code` needs `MOONKALE_TLS_CERT`/`MOONKALE_TLS_KEY` (PEM) off loopback, or `MOONKALE_INSECURE_HTTP=1` behind a TLS proxy; `--token-stdin` reads the token from the first stdin line; `MOONKALE_TERMINAL=0` switches shells off. Tests: `packages/web/tests/e2e/server.mjs` (in `run-all.sh` as `server`), `cargo test -p moonkale-remote --test shim -- --ignored` (fake `ssh`; plus a real-`sshd` variant with `MOONKALE_TEST_SSH`, recipe in `packages/remote/remote.md`), `cargo test -p moonkale-ext-api --test remote_flow`, `MOONKALE_REMOTE=http://127.0.0.1:8090 cargo test -p desktop --test remote -- --ignored`.
- **Build time (spec 023):** a desktop `dx serve` builds the client *and* a server binary; with full debug info both were 1.8 GB and a pull that changes crate features (anything touching `tokio`, `serde`, `dioxus`) cost ~30 min. The root `Cargo.toml` now sets `[profile.dev] debug = "line-tables-only"` and no debug info for dependencies (set `debug = 2` for a session in a debugger). Scripts and agents keep their own `CARGO_TARGET_DIR` so `cargo test`/`clippy` with other feature sets do not invalidate what `dx serve` built into `target/debug`.
- **Graph with several folders (spec 020):** all open folders are drawn in one graph, coloured per folder; the source picker narrows to one. **Markdown opens in Rich mode** (spec 021; Settings → Editor → *Open markdown files in Rich mode*; the E2E fixture turns it off in `.moonkale/settings.json` because the suites drive the source editor). **Julia and Python symbols** in the graph (spec 022).
- **API keys** for the dev shell live in `.secrets/llm.env` (gitignored): `source .secrets/llm.env` before `dx serve` — the file exports `MOONKALE_LLM`, `OPENAI_BASE_URL`, `OPENAI_API_KEY`, `MOONKALE_LLM_MODEL`, `MOONKALE_EMBED_MODEL`. Mistral's API is OpenAI-compatible (`https://api.mistral.ai/v1`, `mistral-code-latest`, `mistral-embed`). `packages/web/tests/e2e/agent-live.mjs` checks a real model against the fixture; it is not part of the regular suite.
- **Milestone 4 agent:** the Agent tab uses `MOONKALE_LLM` / `ANTHROPIC_API_KEY` / `OPENAI_API_KEY` / `OLLAMA_HOST` from the environment of the process that runs the provider (the desktop app, or the server under `dx serve` for web); nothing set = offline mock (`/tool <name> <json>` in the chat calls a tool). `MOONKALE_EMBED_MODEL` turns on embeddings for search. Ctrl+Shift+F searches; **Trace → Graph** in the terminal draws a panic/compiler error.
- The xterm bundle is committed (`packages/editors/terminal/assets/xterm.{js,css}`); rebuild after changing `packages/js/xterm/src` with `npm run build` there.
- The CodeMirror bundle is committed (`packages/editors/code/assets/codemirror.js`); rebuild it after changing `packages/js/codemirror/src` with `npm run build` there.

Strategy and per-layer recipes: [[Testing Strategy]], [[How to Write Tests]].

## Icons and the banner

Source images live in `/assets` (`Moonkale.png` 1254², `Moonkale512.png`, `Moonkale{16,32,64}.{png,ico}`, `MoonkaleBanner.png`). Derived copies — regenerate with ImageMagick when the source changes:

| use | file | from |
|---|---|---|
| desktop/mobile bundle icon (`Dioxus.toml` `icon`), Arch package | `packages/{desktop,mobile}/assets/icon.png` | `Moonkale512.png` |
| desktop window/taskbar icon (no decoder at runtime) | `packages/desktop/assets/icon64.rgba` | `magick assets/Moonkale64.png -depth 8 rgba:…` |
| web favicon (16/32/64 in one `.ico`) + touch icon | `packages/web/assets/{favicon.ico,icon.png}` | `magick Moonkale16.png Moonkale32.png Moonkale64.png favicon.ico` |
| title-bar logo (drawn at 16 px) | `packages/ui/assets/icon32.png` | `Moonkale32.png` |
| website favicon + page-title logo, social preview | `site/quartz/static/{icon.png,og-image.png}` | `-resize 256x256`; `-resize 1200x675^ -gravity center -extent 1200x675` |
| banner on the site index and the README | `markdown/assets/MoonkaleBanner.png` (1400 px, PNG8) | `-resize 1400x -dither None -colors 256` |

## The vault and the website

The repository root is the Obsidian vault (`.obsidian/` at the root); the notes live in `markdown/`. Installed Obsidian plugins — Inline TikZ, Wypst (Typst), Markdown Tabs, Excalidraw, Document Comments — have matching build-time support in the site where it exists (TikZ, Typst, Tabs; Mermaid is built in).

The website <https://mathstruct.github.io/Moonkale/> is built with [Quartz 4](https://quartz.jzhao.xyz), vendored in `site/` from the [MathStruct site](https://github.com/MathStruct/mathstruct.github.io) so both share the same plugins and look. Pushing to `master` runs `.github/workflows/deploy.yml`, which builds `markdown/` and deploys to GitHub Pages.

Preview locally:

```sh
cd site
npm ci                                  # first time only
npx quartz build --serve -d ../markdown # http://localhost:8080, rebuilds on save
```

Conventions:
- `index.md` is the landing page; `Home.md` is the full map of contents.
- Frontmatter: `title` (optional — the filename is used otherwise), `description`, `tags`. ADRs carry `status` and `date`; problems carry `status` and `phase`.
- `templates/` and `Prompt*.md` are not published (`ignorePatterns` in `site/quartz.config.ts`).
- Diagrams: Mermaid fences render everywhere. ```` ```tikz ```` and ```` ```typst ```` fences and `$…$` math also render on the site (Typst first, KaTeX fallback — see the MathStruct [authoring guide](https://mathstruct.github.io/guides/authoring) for the rules).
- Every problem hit during implementation gets a note via [[Problem Template]] and a row in [[Problem Log]].
- Versioning of the vault itself is plain git; versioning *inside* Moonkale is described in [[Version Management]].

## Packaging

How the app becomes a package — Arch PKGBUILD, Nix flake, Android APK/AAB — is in [[Packaging Overview]]. Short version: always build with `dx build --release --platform linux --package desktop` (never bare `cargo build`: assets are collected by dx), then install `moonkale` to `bin/` and `assets/` to `lib/Moonkale/`. The desktop binary is named `moonkale` (`[[bin]]` in `packages/desktop/Cargo.toml`) for exactly this reason.

## Repository hygiene
- `.obsidian/workspace.json` is ignored (per-user state); plugin folders are tracked so a fresh clone opens with the same plugins.
- `site/.tikz-cache/` is tracked on purpose: it keeps CI from recompiling every TikZ diagram on each build.
