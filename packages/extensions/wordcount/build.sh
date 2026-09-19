#!/usr/bin/env bash
# Builds the example wasm extension and installs it for this user.
set -euo pipefail
cd "$(dirname "$0")/../../.."
cargo build -p moonkale-ext-wordcount --target wasm32-unknown-unknown --release
DEST="${MOONKALE_CONFIG_DIR:-${XDG_CONFIG_HOME:-$HOME/.config}/moonkale}/extensions"
mkdir -p "$DEST"
cp target/wasm32-unknown-unknown/release/moonkale_ext_wordcount.wasm "$DEST/wordcount.wasm"
ls -la "$DEST/wordcount.wasm"
