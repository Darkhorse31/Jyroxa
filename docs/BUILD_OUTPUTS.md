# Jyroxa build outputs

Release artifacts are written under `dist/` (git-ignored). Typical names for **0.1.4**:

| Platform | Artifact |
| -------- | -------- |
| Windows NSIS | `dist/Jyroxa_0.1.4_x64-setup.exe` |
| Windows portable | `dist/Jyroxa-0.1.4-windows-x64-portable.zip` |
| Browser extension | `dist/jyroxa-browser-extension.zip` |
| Android universal APK | `dist/Jyroxa-0.1.4-android-universal.apk` |

Gradle copies the signed release APK via `./gradlew :app:exportReleaseApk`; release staging adds `-android-` to match the published artifact name.

## Android (release APK)

| Artifact | Path |
| -------- | ---- |
| Copied distribution name | `dist/Jyroxa-<version>-universal.apk` |
| Gradle output | `android/app/build/outputs/apk/release/app-release.apk` |

JNI libraries (`libjyroxa_ffi.so`) are built into `android/app/src/main/jniLibs/` by `:app:buildCargoNdk` (git-ignored per `android/.gitignore`).

## Desktop (Windows GNU)

| Artifact | Path |
| -------- | ---- |
| Release executable | `target/release/jyroxa-desktop.exe` |
| WebView2 loader (required beside exe on GNU) | `target/release/WebView2Loader.dll` |
| Redistributable copy (tracked) | `apps/desktop/src-tauri/WebView2Loader.dll` |
| NSIS installer | `target/release/bundle/nsis/Jyroxa_0.1.4_x64-setup.exe` |
| Native messaging host | `target/release/jyroxa-native-host.exe` (bundled beside desktop via `tauri.conf.json` resources) |

Build the native host before the Tauri bundle: `cargo build --release -p jyroxa-native-host`.

Portable zip: run `apps/desktop/package-portable.ps1` after `build-gnu.ps1` (includes `jyroxa-native-host.exe` when present).

Browser extension zip: `scripts/package-browser-ext.ps1`.

**Note:** On GNU builds, `jyroxa-desktop.exe` requires `WebView2Loader.dll` beside it (installer, portable zip, or NSIS bundle).

## Workspace Rust binaries

| Artifact | Path |
| -------- | ---- |
| CLI | `target/release/jyroxa.exe` |
