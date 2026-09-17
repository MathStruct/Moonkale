{
  description = "Moonkale — graph-native code and knowledge editor";

  # Status: written against dioxus-cli 0.7.10's output layout; not yet built
  # under Nix. See markdown/packaging/NixOS.md for the reasoning and caveats
  # (dx must run inside the sandbox because plain cargo does not collect the
  # hashed `asset!()` files).

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
            cd packages/desktop
            dx build --release --platform linux --package desktop --cargo-args "--frozen"
            cd ../..
            runHook postBuild
          '';

          installPhase = ''
            runHook preInstall
            app=target/dx/desktop/release/linux/app
            install -Dm755 "$app/moonkale" "$out/bin/moonkale"
            mkdir -p "$out/lib/Moonkale"
            cp -r "$app/assets" "$out/lib/Moonkale/assets"
            install -Dm644 packaging/linux/moonkale.desktop "$out/share/applications/moonkale.desktop"
            runHook postInstall
          '';

          # No `cargo test` here: the workspace tests are run by `nix flake check` below.
          doCheck = false;

          meta = with pkgs.lib; {
            description = "Graph-native code and knowledge editor";
            homepage = "https://github.com/MathStruct/Moonkale";
            license = with licenses; [ mit asl20 ];
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
          checkPhase = "cargo test --workspace --offline";
          installPhase = "mkdir -p $out";
        };
      });
}
