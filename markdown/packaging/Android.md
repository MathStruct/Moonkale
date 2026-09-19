---
title: "Android"
description: Building, signing and distributing Moonkale's Android app with dx.
tags: [packaging, android]
---
Crate: `packages/mobile`. Config: `packages/mobile/Dioxus.toml`. Background: [[Packaging Overview]]. What dx does was checked in `dioxus-cli` 0.7.10 (`src/build/android.rs`, `assets/android/gen/**`). Official guide: <https://dioxuslabs.com/learn/0.7/guides/platforms/mobile/>.

## What dx does for Android

`dx build --platform android --package mobile` generates a **Gradle project** under `target/dx/mobile/<debug|release>/android/app/` from its own templates (Kotlin `MainActivity`, `AndroidManifest.xml` with `INTERNET` already granted, `build.gradle.kts`), compiles the Rust crate as a shared library for each requested ABI into `app/src/main/jniLibs/<abi>/`, copies assets, and runs `./gradlew` to produce:

```text
target/dx/mobile/debug/android/app/app/build/outputs/apk/debug/app-debug.apk
target/dx/mobile/release/android/app/app/build/outputs/apk/release/app-release.apk
```

`[application] name` is the launcher label; `[bundle] identifier` (`io.github.mathstruct.moonkale`) is the package id; `[android] min_sdk = 24` (dx's default) / `target_sdk = 35`.

## Toolchain (Arch)

```sh
# JDK 17 (the generated Gradle project targets JVM 17)
sudo pacman -S jdk17-openjdk
# Android SDK + NDK: either Android Studio (AUR: android-studio) and its SDK Manager,
# or the command-line tools (AUR: android-sdk-cmdline-tools-latest) and:
sdkmanager "platform-tools" "platforms;android-35" "build-tools;35.0.0" "ndk;27.2.12479018"
# Rust targets
rustup target add aarch64-linux-android      # phones
rustup target add x86_64-linux-android       # emulator
```

Environment (dx reads these names): `ANDROID_HOME` (or `ANDROID_SDK_ROOT`), `ANDROID_NDK_HOME` (or `NDK_HOME`), `JAVA_HOME`.

```sh
export ANDROID_HOME=$HOME/Android/Sdk
export ANDROID_NDK_HOME=$ANDROID_HOME/ndk/27.2.12479018
export JAVA_HOME=/usr/lib/jvm/java-17-openjdk
```

## Develop

```sh
cd packages/mobile
adb devices                                   # a device with USB debugging, or a running emulator
dx serve --platform android                   # builds, installs, launches; hot-reloads rsx/assets
```

dx uses `adb reverse` so the app reaches the dev server on `127.0.0.1` ([[Debugging and Logging]] for `adb logcat`).

> [!tip] What actually worked (2026-09-19, Galaxy S10e / Android 13, NDK 29.0.14206865)
> `dx build --release --platform android --features mobile --target aarch64-linux-android` then `adb install -r target/dx/mobile/release/android/app/app/build/outputs/apk/debug/app-debug.apk`. The plain debug build is x86_64 (`INSTALL_FAILED_NO_MATCHING_ABIS`) and too large for a phone with 1.4 GB free (`INSTALL_FAILED_INSUFFICIENT_STORAGE`); the release APK is 23 MB. Launch with `adb shell monkey -p io.github.mathstruct.moonkale -c android.intent.category.LAUNCHER 1`; inspect with `adb exec-out screencap -p`, `adb forward tcp:9222 localabstract:webview_devtools_remote_<pid>` (WebView DevTools) and `adb shell run-as io.github.mathstruct.moonkale`. The five WebView-specific fixes are P-087–P-091 in the [[Problem Log]]; the recipe lives in `packages/mobile/README.md`.

## Release APK / AAB

1. Create a keystore once, **outside the repo**:
   ```sh
   keytool -genkeypair -v -keystore ~/.keys/moonkale-release.jks -alias moonkale \
           -keyalg RSA -keysize 4096 -validity 10000
   ```
2. Fill `[android.signing]` in `packages/mobile/Dioxus.toml` — but do not commit passwords. Two workable patterns:
   - CI writes the block into `Dioxus.toml` from secrets right before `dx bundle` (the file is not committed in that state);
   - locally, keep a `.gitignore`d copy `Dioxus.toml` and a committed `Dioxus.toml.example` (dx reads only `Dioxus.toml`, it has no env-var substitution as of 0.7.10).
3. Build:
   ```sh
   cd packages/mobile
   dx bundle --release --platform android --package-types apk     # sideload / GitHub release
   dx bundle --release --platform android --package-types aab     # Google Play
   ```
   Without `[android.signing]`, a release build still produces the **debug-signed** APK path; dx only switches to `app-release.apk` when signing is configured (`android_apk_path()` in the CLI).
4. Verify: `apksigner verify --print-certs app-release.apk`, then `adb install -r app-release.apk`.

## Distribution channels

| channel | needs | notes |
|---|---|---|
| GitHub Releases (sideload) | signed APK | simplest; users enable unknown sources |
| Google Play | AAB, Play Console account, `target_sdk` per current Play policy | dx produces the AAB; upload manually or with `fastlane supply` |
| F-Droid | reproducible build from source on their server | Rust + NDK builds are supported via `srclibs`; needs a metadata YAML and no proprietary deps. Blocked on: an icon, a tagged release, and confirming the Gradle project dx generates builds offline |

## Extensions, app size and the stores
[[Android Extensions and Bundling]] answers what can be added after install (wasm extensions from a URL, once the phone has a runtime and an installer — never native code), how small the APK can get (strip + a clean asset dir ≈ 15 MB, feature-trimmed ≈ 12 MB), and what Google Play and F-Droid require. Before a release build, delete `target/dx/mobile/release/android/app/app/src/main/assets` — dx keeps every previous hashed asset (P-092).

## What Moonkale needs before Android is *useful*
- **Folder access**: `open_local` uses `FolderSource::open(".")`, which on Android is the app sandbox. Real folders need the Storage Access Framework (P-25 in [[Problem Ranking]]).
- **Layout**: the workbench is desktop-shaped; the collapsed mobile shell is Phase 5.
- **Remote mode** ([[ADR-0005 Server functions as the remote backend]]) is the realistic v1 on a phone: point the app at a Moonkale server.
- An icon (`bundle.icon`), and `permissions` in `Dioxus.toml` once storage is used.

## First-run checklist
- [ ] `dx serve --platform android` shows the shell on an emulator
- [ ] `dx bundle --release --platform android --package-types apk` produces `app-release.apk` (signing configured)
- [ ] the APK installs and opens on a device; log the result in [[Problem Log]]
