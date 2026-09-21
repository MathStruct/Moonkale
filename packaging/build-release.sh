#!/usr/bin/env bash
# Builds the desktop app and the server (release) and turns them into the
# Linux artefacts (Milestone 13): one staging tree, then a tarball, a .deb
# and — when makepkg is around — an Arch binary package.
#
#   packaging/build-release.sh              # everything into dist/
#   packaging/build-release.sh --no-build   # re-package what target/ already has
#   packaging/build-release.sh --no-arch    # skip makepkg (CI on Debian/Ubuntu)
#
# Needs: dx (dioxus-cli 0.7.10), cargo, tar, ar (binutils), gzip; makepkg
# for the Arch package. No Debian tooling: the .deb is assembled by hand.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO"

BUILD=1; ARCH_PKG=1
for a in "$@"; do
  case "$a" in
    --no-build) BUILD=0 ;;
    --no-arch) ARCH_PKG=0 ;;
    *) echo "unknown flag $a" >&2; exit 2 ;;
  esac
done

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
case "$(uname -m)" in
  x86_64) ARCH=x86_64; DEB_ARCH=amd64 ;;
  aarch64|arm64) ARCH=aarch64; DEB_ARCH=arm64 ;;
  *) ARCH="$(uname -m)"; DEB_ARCH="$ARCH" ;;
esac
NAME="moonkale-$VERSION-linux-$ARCH"
DIST="$REPO/dist"
STAGE="$DIST/$NAME"
APP="$REPO/target/dx/moonkale/release/linux/app"
SERVER="$REPO/target/dx/web/release/web/server"

if [ "$BUILD" = 1 ]; then
  echo "== building the desktop app (release)"
  (cd packages/desktop && dx build --platform desktop --release --features desktop)
  echo "== building the server (release)"
  (cd packages/web && dx build --platform server --release)
fi
[ -x "$APP/moonkale" ] || { echo "no desktop build at $APP (run without --no-build)" >&2; exit 1; }
[ -x "$SERVER" ] || { echo "no server build at $SERVER" >&2; exit 1; }

echo "== staging $STAGE"
rm -rf "$STAGE"
mkdir -p "$STAGE/bin" "$STAGE/lib/Moonkale" "$STAGE/share/applications" \
  "$STAGE/share/icons/hicolor/512x512/apps" "$STAGE/share/doc/moonkale"
# The asset resolver looks for lib/<ProductName>/assets next to bin/ (Packaging Overview).
install -m755 "$APP/moonkale" "$STAGE/bin/moonkale"
cp -r "$APP/assets" "$STAGE/lib/Moonkale/assets"
# The server: what Open Remote Folder… uploads, and `moonkale-server` on its own.
install -m755 "$SERVER" "$STAGE/bin/moonkale-server"
strip "$STAGE/bin/moonkale" "$STAGE/bin/moonkale-server" 2>/dev/null || true
install -m644 packaging/linux/moonkale.desktop "$STAGE/share/applications/moonkale.desktop"
install -m644 packages/desktop/assets/icon.png "$STAGE/share/icons/hicolor/512x512/apps/moonkale.png"
install -m644 README.md LICENSE "$STAGE/share/doc/moonkale/"
# Third-party notices: every crate with its licence (cargo-license when
# installed; the vault's Licensing page otherwise).
{
  echo "Moonkale $VERSION — third-party software in this package"
  echo
  echo "Moonkale itself is MIT (LICENSE). The binaries also contain the crates"
  echo "and the JavaScript bundles listed below, under their own licences."
  echo "Overview: https://mathstruct.github.io/Moonkale/platform/Licensing"
  echo
  if command -v cargo-license >/dev/null 2>&1; then
    cargo license --avoid-build-deps --avoid-dev-deps 2>/dev/null || true
  else
    echo "(install cargo-license and rerun for the full crate list)"
  fi
  echo
  echo "JavaScript bundles: CodeMirror (MIT), Milkdown/ProseMirror/remark (MIT), xterm.js (MIT), KaTeX (MIT), KaTeX fonts (SIL OFL 1.1)."
} > "$STAGE/share/doc/moonkale/THIRD-PARTY.md"
cat > "$STAGE/install.sh" <<'EOF'
#!/bin/sh
# Installs this unpacked Moonkale into a prefix (default /usr/local; use ~/.local for your user).
set -e
PREFIX="${1:-/usr/local}"
HERE="$(cd "$(dirname "$0")" && pwd)"
mkdir -p "$PREFIX/bin" "$PREFIX/lib" "$PREFIX/share"
cp -r "$HERE/bin/." "$PREFIX/bin/"
rm -rf "$PREFIX/lib/Moonkale"; cp -r "$HERE/lib/Moonkale" "$PREFIX/lib/Moonkale"
cp -r "$HERE/share/." "$PREFIX/share/"
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$PREFIX/share/applications" || true
command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q "$PREFIX/share/icons/hicolor" || true
echo "installed to $PREFIX — run: $PREFIX/bin/moonkale"
EOF
chmod +x "$STAGE/install.sh"

echo "== tarball"
tar -C "$DIST" -czf "$DIST/$NAME.tar.gz" "$NAME"

echo "== deb"
DEB_DIR="$DIST/deb"
rm -rf "$DEB_DIR"; mkdir -p "$DEB_DIR/data/usr" "$DEB_DIR/control"
cp -r "$STAGE/bin" "$STAGE/lib" "$STAGE/share" "$DEB_DIR/data/usr/"
DEPENDS="$(sed -n 's/^depends = \[\(.*\)\]/\1/p' packages/desktop/Dioxus.toml | tr -d '"' | sed 's/, */, /g')"
SIZE_KB="$(du -sk "$DEB_DIR/data" | cut -f1)"
cat > "$DEB_DIR/control/control" <<EOF
Package: moonkale
Version: $VERSION
Architecture: $DEB_ARCH
Maintainer: Daniel Boigk <daniel@mathstruct.org>
Installed-Size: $SIZE_KB
Depends: $DEPENDS
Section: devel
Priority: optional
Homepage: https://github.com/MathStruct/Moonkale
Description: Graph-native code and knowledge editor
 Moonkale opens folders and databases as one graph and edits them with
 dockable code, markdown, table, graph and flow editors. Includes
 moonkale-server for remote folders and self-hosting.
EOF
cat > "$DEB_DIR/control/postinst" <<'EOF'
#!/bin/sh
set -e
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q /usr/share/applications || true
command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q /usr/share/icons/hicolor || true
EOF
chmod 755 "$DEB_DIR/control/postinst"
( cd "$DEB_DIR/data" && find . -type f -exec md5sum {} \; | sed 's| \./| |' > "$DEB_DIR/control/md5sums" )
( cd "$DEB_DIR/control" && tar --owner=0 --group=0 -czf ../control.tar.gz ./control ./postinst ./md5sums )
( cd "$DEB_DIR/data" && tar --owner=0 --group=0 -czf ../data.tar.gz . )
echo "2.0" > "$DEB_DIR/debian-binary"
DEB="$DIST/moonkale_${VERSION}_${DEB_ARCH}.deb"
rm -f "$DEB"
( cd "$DEB_DIR" && ar rcs "$DEB" debian-binary control.tar.gz data.tar.gz )
rm -rf "$DEB_DIR"

if [ "$ARCH_PKG" = 1 ] && command -v makepkg >/dev/null 2>&1; then
  echo "== Arch package (moonkale-bin)"
  ( cd packaging/arch-bin && rm -f "$NAME.tar.gz" && cp "$DIST/$NAME.tar.gz" . \
    && MOONKALE_VERSION="$VERSION" makepkg -f --skipchecksums \
    && mv moonkale-bin-*.pkg.tar.* "$DIST/" && rm -rf pkg src "$NAME.tar.gz" )
fi

echo "== checksums"
( cd "$DIST" && sha256sum *.tar.gz *.deb *.pkg.tar.* 2>/dev/null > sha256sums.txt || true; cat sha256sums.txt )
echo "== done: $DIST"
ls -la "$DIST" | grep -vE "^d|^total"
