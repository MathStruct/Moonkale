---
title: "NixOS"
description: Packaging Moonkale for Nix/NixOS with a flake — package, dev shell, checks.
tags: [packaging, nix]
---
Recipe: `flake.nix` at the repo root. Background: [[Packaging Overview]].

## What the flake provides

| output | command | what |
|---|---|---|
| `packages.default` | `nix build` → `result/bin/moonkale` | the desktop app, FHS layout under `$out` |
| `devShells.default` | `nix develop` | cargo, `dx`, `wasm-bindgen-cli`, Node 22, GTK/WebKit libs — enough for `dx serve` on desktop and web |
| `checks.default` | `nix flake check` | `cargo test --workspace` in the sandbox |

## Why the derivation is shaped this way

- **`rustPlatform.buildRustPackage` with `cargoLock.lockFile`** — vendors crates from the committed `Cargo.lock`; no `cargoHash` to keep updating. Works because every dependency is on crates.io (no git deps).
- **`dx build` inside `buildPhase`** — plain `cargo build` would leave the binary without its hashed assets ([[Packaging Overview]]). `dioxus-cli` comes from nixpkgs (`pkgs.dioxus-cli`) and goes in `nativeBuildInputs`. dx spawns `cargo`, which picks up the vendored-source config `buildRustPackage` wrote, so the sandbox's no-network rule holds. `HOME=$TMPDIR` because dx keeps caches under `$HOME`.
- **`wrapGAppsHook3`** — wraps the binary so GTK finds its schemas, GIO modules and the WebKitGTK process binaries; without it a Nix-built GTK app typically starts with a blank window or GSettings errors.
- **`installPhase`** copies `target/dx/desktop/release/linux/app/{moonkale,assets}` into `$out/bin` and `$out/lib/Moonkale/assets` — the layout the asset resolver checks first.
- **`doCheck = false`** in the package, tests in a separate `checks` derivation, so `nix build` doesn't rebuild the world twice.

## Build, run, install

```sh
nix build               # → ./result/bin/moonkale
./result/bin/moonkale
nix run                 # same, without keeping a result link
nix develop             # dev shell for dx serve
nix flake check         # workspace tests

# NixOS system config (flake-based):
#   inputs.moonkale.url = "github:MathStruct/Moonkale";
#   environment.systemPackages = [ inputs.moonkale.packages.${system}.default ];
```

## Things to verify on first use (none of this has run yet)
1. `nix eval nixpkgs#dioxus-cli.version` ≥ `0.7.10`. If nixpkgs lags, pin a newer nixpkgs for that one package or build `dioxus-cli` with `buildRustPackage` from crates.io inside the flake.
2. dx must not try to download anything for a Linux desktop build (it downloads `wasm-bindgen`/`esbuild` only for web). If it does, the sandbox fails with a network error — report it as a problem note.
3. The NVIDIA/WebKitGTK caveats in [[Linux Desktop Setup]] apply to the Nix build too; the dev shell has the `WEBKIT_DISABLE_DMABUF_RENDERER` line ready to uncomment.
4. `wrapGAppsHook3` vs a plain binary: if the app starts but WebKit shows nothing, check that `WEBKIT_EXEC_PATH`/`GIO_EXTRA_MODULES` are set by the wrapper (`cat result/bin/moonkale` shows the wrapper script).

## Android on NixOS
`androidenv.composeAndroidPackages` can provide SDK + NDK declaratively; export `ANDROID_HOME`/`ANDROID_NDK_HOME` from it in a dev shell and follow [[Android]]. Not wired into the flake yet.
