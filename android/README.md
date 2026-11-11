# Jyroxa Android app

Kotlin + Jetpack Compose (Material 3) frontend for **Jyroxa**, with JNI into `crates/jyroxa-ffi` (`libjyroxa_ffi.so`).

- Package: `com.jyroxa.app`
- Native entry: `com.jyroxa.app.JyroxaFfi` ↔ `Java_com_jyroxa_app_JyroxaFfi_*`
- High-level bridge: `com.jyroxa.app.native.JyroxaNative`

## Prerequisites

- JDK 17+
- Android SDK (API 34, build-tools 34.0.0+), NDK 27.x
- Rust + `cargo-ndk`, targets `aarch64-linux-android`, `armv7-linux-androideabi`, `x86_64-linux-android`
- `android/local.properties` with `sdk.dir=` (git-ignored)

On Windows GNU hosts, clear `RUSTC_WRAPPER` when invoking `cargo ndk` for Android.

## Build native library

```bash
cd android
./gradlew buildCargoNdk
```

Outputs under `app/src/main/jniLibs/{armeabi-v7a,arm64-v8a,x86_64}/libjyroxa_ffi.so` (git-ignored).

## Build signed release APK

Configure signing in git-ignored `android/keystore.properties` (see template in this file’s history). Then:

```bash
./gradlew :app:assembleRelease :app:exportReleaseApk
```

Distribution copy: `../dist/Jyroxa-<version>-universal.apk`.

## Debug

```bash
./gradlew :app:assembleDebug
# APK: app/build/outputs/apk/debug/app-debug.apk
```

## Keystore (local only)

Never commit keystores or passwords. Example `keystore.properties`:

```properties
storeFile=keystore/jyroxa-release.jks
storePassword=<store-password>
keyAlias=jyroxa
keyPassword=<key-password>
```

Generate a keystore once with `keytool`; keep files under `android/keystore/` (git-ignored).
