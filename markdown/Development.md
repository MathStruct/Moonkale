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
- The CodeMirror bundle is committed (`packages/editors/code/assets/codemirror.js`); rebuild it after changing `packages/js/codemirror/src` with `npm run build` there.

Strategy and per-layer recipes: [[Testing Strategy]], [[How to Write Tests]].

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

## Packaging

How the app becomes a package — Arch PKGBUILD, Nix flake, Android APK/AAB — is in [[Packaging Overview]]. Short version: always build with `dx build --release --platform linux --package desktop` (never bare `cargo build`: assets are collected by dx), then install `moonkale` to `bin/` and `assets/` to `lib/Moonkale/`. The desktop binary is named `moonkale` (`[[bin]]` in `packages/desktop/Cargo.toml`) for exactly this reason.

## Repository hygiene
- `.obsidian/workspace.json` is ignored (per-user state); plugin folders are tracked so a fresh clone opens with the same plugins.
- `site/.tikz-cache/` is tracked on purpose: it keeps CI from recompiling every TikZ diagram on each build.
