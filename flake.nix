{
  description = "Moonkale — graph-native code and knowledge editor";

  # Written against dioxus-cli 0.7.10's output layout (target/dx/moonkale/…,
  # Milestone 13); dx must run inside the sandbox because plain cargo does not
  # collect the hashed `asset!()` files. See markdown/packaging/NixOS.md.
  # Verified in CI (release.yml) — the dev box's nix-daemon is off.

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        # Runtime libraries the desktop binary links (matches the -l flags of a
        # dioxus-desktop link line on Linux).
        runtimeLibs = with pkgs; [ webkitgtk_4_1 gtk3 libayatana-appindicator xdotool openssl ];
        # LadybugDB's `lbug` crate downloads its prebuilt C++ library at build
        # time (no network in the sandbox; from source it needs CMake and an
        # hour). Fetch the same archive as a fixed-output derivation and hand
        # it to the build script through LBUG_LIBRARY_DIR / LBUG_INCLUDE_DIR.
        lbugVersion = "0.20.4";
        liblbug = pkgs.stdenv.mkDerivation {
          pname = "liblbug-prebuilt";
          version = lbugVersion;
          src = pkgs.fetchurl {
            url = "https://github.com/LadybugDB/ladybug/releases/download/v${lbugVersion}/liblbug-static-linux-x86_64-compat.tar.gz";
            hash = "sha256-eZ8Y8WX6FQdbBJ1x6EKeD/n3aIvl0YbElz0RGmmAUJo=";
          };
          sourceRoot = ".";
          installPhase = ''
            mkdir -p $out/lib
            cp -r . $out/lib/
          '';
        };
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "moonkale";
          version = "0.1.0";
          src = self;
          # No cargoHash to keep in sync: vendor from the committed lockfile.
          cargoLock.lockFile = ./Cargo.lock;

          nativeBuildInputs = with pkgs; [ pkg-config wrapGAppsHook3 dioxus-cli ];
          buildInputs = runtimeLibs;

          # dx must be >= 0.7.10 (the workspace's dioxus version); check with
          # `nix eval nixpkgs#dioxus-cli.version` and override the input if older.
          buildPhase = ''
            runHook preBuild
            export HOME=$TMPDIR                # dx writes caches under $HOME
            export CARGO_NET_OFFLINE=true
            export LBUG_LIBRARY_DIR=${liblbug}/lib
            export LBUG_INCLUDE_DIR=${liblbug}/lib
            (cd packages/desktop && dx build --release --platform desktop --features desktop --cargo-args=--frozen)
            (cd packages/web && dx build --release --platform server --cargo-args=--frozen)
            runHook postBuild
          '';

          installPhase = ''
            runHook preInstall
            app=target/dx/moonkale/release/linux/app
            install -Dm755 "$app/moonkale" "$out/bin/moonkale"
            install -Dm755 target/dx/web/release/web/server "$out/bin/moonkale-server"
            mkdir -p "$out/lib/Moonkale"
            cp -r "$app/assets" "$out/lib/Moonkale/assets"
            install -Dm644 packaging/linux/moonkale.desktop "$out/share/applications/moonkale.desktop"
            install -Dm644 packages/desktop/assets/icon.png "$out/share/icons/hicolor/512x512/apps/moonkale.png"
            runHook postInstall
          '';

          # No `cargo test` here: the workspace tests are run by `nix flake check` below.
          doCheck = false;

          meta = with pkgs.lib; {
            description = "Graph-native code and knowledge editor";
            homepage = "https://github.com/MathStruct/Moonkale";
            license = licenses.mit;
            mainProgram = "moonkale";
            platforms = platforms.linux;
          };
        };

        # `nix develop`: everything needed for `dx serve` on desktop and web.
        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            cargo rustc rustfmt clippy rust-analyzer
            dioxus-cli wasm-bindgen-cli
            pkg-config nodejs_22
          ] ++ runtimeLibs;
          # WebKitGTK on NVIDIA/Wayland: see markdown/platform/Linux Desktop Setup.md
          # WEBKIT_DISABLE_DMABUF_RENDERER = "1";
        };

        checks.default = pkgs.rustPlatform.buildRustPackage {
          pname = "moonkale-tests";
          version = "0.1.0";
          src = self;
          cargoLock.lockFile = ./Cargo.lock;
          nativeBuildInputs = with pkgs; [ pkg-config ];
          buildInputs = runtimeLibs;
          buildPhase = "true";
          LBUG_LIBRARY_DIR = "${liblbug}/lib";
          LBUG_INCLUDE_DIR = "${liblbug}/lib";
          checkPhase = "cargo test --workspace --exclude mobile --offline";
          installPhase = "mkdir -p $out";
        };
      });
}
