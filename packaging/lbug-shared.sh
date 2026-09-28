#!/usr/bin/env bash
# LadybugDB as a *shared* library (Milestone 17).
#
# The `lbug` crate links a prebuilt static liblbug.a by default. That archive
# carries LadybugDB's bundled zstd and SimSIMD with global symbols, which
# clash with the zstd-sys and simsimd crates that Turso, HelixDB and RocksDB
# bring (184 duplicate symbols). The shared liblbug keeps zstd to itself, so
# with it everything links. This script fetches the shared library for this
# machine (the version in Cargo.lock), caches it, and runs a command with the
# variables the `lbug` build script reads:
#
#   packaging/lbug-shared.sh dx serve --features ladybug          # desktop dev
#   packaging/lbug-shared.sh cargo build -p desktop --features desktop,ladybug
#   packaging/lbug-shared.sh dir                                    # print the directory
#
# Binaries built this way find the library through an rpath to the cache
# directory; packages ship it themselves (packaging/arch/PKGBUILD).
set -euo pipefail
REPO="$(cd "$(dirname "$0")/.." && pwd)"

VERSION="${LBUG_VERSION_OVERRIDE:-$(awk '/^name = "lbug"$/ { getline; gsub(/version = |"/, ""); print; exit }' "$REPO/Cargo.lock")}"
[ -n "$VERSION" ] || { echo "no lbug in Cargo.lock" >&2; exit 1; }

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) PLATFORM=linux-x86_64 ;;
  Linux-aarch64) PLATFORM=linux-aarch64 ;;
  Darwin-arm64) PLATFORM=osx-arm64 ;;
  Darwin-x86_64) PLATFORM=osx-x86_64 ;;
  *) echo "no shared liblbug for $(uname -s)-$(uname -m)" >&2; exit 1 ;;
esac

DIR="${XDG_CACHE_HOME:-$HOME/.cache}/moonkale/lbug-shared/$VERSION/$PLATFORM"
if [ ! -f "$DIR/lbug.h" ]; then
  mkdir -p "$DIR"
  url="https://github.com/LadybugDB/ladybug/releases/download/v$VERSION/liblbug-$PLATFORM.tar.gz"
  echo "lbug-shared: fetching $url" >&2
  curl -fsSL "$url" | tar -xz -C "$DIR"
  # The linker looks for liblbug.so / liblbug.dylib; the loader for the soname.
  if lib=$(ls "$DIR"/liblbug.so.* 2>/dev/null | head -1); then
    ln -sf "$(basename "$lib")" "$DIR/liblbug.so"
    soname=$(readelf -d "$lib" 2>/dev/null | sed -n 's/.*SONAME.*\[\(.*\)\]/\1/p')
    [ -n "$soname" ] && [ ! -e "$DIR/$soname" ] && ln -sf "$(basename "$lib")" "$DIR/$soname"
  fi
fi

if [ "${1:-}" = dir ]; then
  echo "$DIR"
  exit 0
fi
[ $# -gt 0 ] || { sed -n '2,15p' "$0"; exit 2; }
export LBUG_SHARED=1 LBUG_LIBRARY_DIR="$DIR" LBUG_INCLUDE_DIR="$DIR"
exec "$@"
