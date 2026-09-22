#!/usr/bin/env bash
# Builds the release arm64 APK, then sets the app label and replaces the
# launcher icon (dx 0.7.10 labels the app after the crate name and always
# writes its own default icons into the generated Gradle project — spec 007)
# and reassembles. A `[[bin]] name = "moonkale"` would fix the label but moves
# the output under target/dx/moonkale/, which the desktop crate already uses. Result: target/dx/mobile/release/android/app/app/build/outputs/apk/debug/app-debug.apk
#
#   packages/mobile/build-android.sh            # build + icon + assemble
#   packages/mobile/build-android.sh install    # … and adb install -r + launch
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Android/Sdk}"
NDK_DIR="$(ls -d "$ANDROID_HOME"/ndk/* | sort -V | tail -1)"
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$NDK_DIR}" NDK_HOME="${NDK_HOME:-$NDK_DIR}"
export JAVA_HOME="${JAVA_HOME:-/usr/lib/jvm/java-17-openjdk}"
export PATH="$ANDROID_HOME/platform-tools:$PATH"
PROJECT="$ROOT/target/dx/mobile/release/android/app"
RES="$PROJECT/app/src/main/res"
ICON="$HERE/assets/icon.png"

# Stale hashed assets accumulate in the generated project (P-092): start clean.
rm -rf "$PROJECT/app/src/main/assets"
# DX_BUILD_ARGS: extra flags for dx (CI passes -v; dx hides cargo errors otherwise).
(cd "$HERE" && dx build --release --platform android --features mobile --target aarch64-linux-android ${DX_BUILD_ARGS:-})

# Launcher icon: bitmaps for every density plus an adaptive icon whose
# foreground is the same picture inset (Android scales the safe zone).
python3 - "$ICON" "$RES" <<'PY'
import sys, os
from PIL import Image
icon, res = sys.argv[1], sys.argv[2]
src = Image.open(icon).convert("RGBA")
for d, px in {"mdpi": 48, "hdpi": 72, "xhdpi": 96, "xxhdpi": 144, "xxxhdpi": 192}.items():
    os.makedirs(f"{res}/mipmap-{d}", exist_ok=True)
    src.resize((px, px), Image.LANCZOS).save(f"{res}/mipmap-{d}/ic_launcher.webp", "WEBP", lossless=True)
    # Adaptive foreground: 108dp canvas, picture in the inner 72dp.
    fg_px = int(px * 108 / 48)
    canvas = Image.new("RGBA", (fg_px, fg_px), (0, 0, 0, 0))
    inner = int(fg_px * 66 / 108)
    canvas.paste(src.resize((inner, inner), Image.LANCZOS), ((fg_px - inner) // 2, (fg_px - inner) // 2))
    canvas.save(f"{res}/mipmap-{d}/ic_launcher_foreground.webp", "WEBP", lossless=True)
# Background colour: the icon's corner pixel (its own backdrop).
r, g, b, _ = src.getpixel((2, 2))
os.makedirs(f"{res}/values", exist_ok=True)
open(f"{res}/values/ic_launcher_background.xml", "w").write(
    f'<?xml version="1.0" encoding="utf-8"?>\n<resources>\n    <color name="ic_launcher_background">#{r:02x}{g:02x}{b:02x}</color>\n</resources>\n')
os.makedirs(f"{res}/mipmap-anydpi-v26", exist_ok=True)
open(f"{res}/mipmap-anydpi-v26/ic_launcher.xml", "w").write(
    '<?xml version="1.0" encoding="utf-8"?>\n<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">\n'
    '    <background android:drawable="@color/ic_launcher_background" />\n'
    '    <foreground android:drawable="@mipmap/ic_launcher_foreground" />\n</adaptive-icon>\n')
# The template's vector background/foreground would shadow ours: drop them.
for f in ["drawable/ic_launcher_background.xml", "drawable-v24/ic_launcher_foreground.xml"]:
    p = f"{res}/{f}"
    if os.path.exists(p):
        os.remove(p)
# App label (dx writes "Mobile", the PascalCase crate name).
sx = f"{res}/values/strings.xml"
open(sx, "w").write('<resources>\n    <string name="app_name">Moonkale</string>\n</resources>\n')
print("icons + label written to", res)
PY

(cd "$PROJECT" && ./gradlew --quiet assembleDebug)
APK="$PROJECT/app/build/outputs/apk/debug/app-debug.apk"
ls -la "$APK"
if [ "${1:-}" = "install" ]; then
  adb install -r "$APK"
  adb shell monkey -p io.github.mathstruct.moonkale -c android.intent.category.LAUNCHER 1 >/dev/null
fi
