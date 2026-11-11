# SwiftGet Android App

Modern, high-performance Android download manager frontend for **SwiftGet**, engineered with **Kotlin**, **Jetpack Compose (Material 3 with Dynamic Color)**, and native **Rust FFI bindings via JNI**.

SwiftGet Android brings desktop-grade download acceleration to mobile devices:
- **Multi-Part Parallel Segments:** Dynamic connection chunking (1–32 parallel threads) with per-segment progress visualization, outperforming standard single-stream mobile downloaders.
- **Real-Time Speed Graph:** High-frequency Canvas speed graph tracking network throughput, peak rates, and transfer curves.
- **JDownloader-style LinkGrabber:** Extract, scrape, and probe links from shared text, clipboard clips, or webpage dumps with category filtering (Videos, Audio, Archives, Documents, Images, Apps).
- **Background Foreground Service (`dataSync`):** Resilient downloading service keeping transfers alive with screen off, accompanied by rich notification controls (Pause All, Resume All, Cancel).
- **Deep Android Integration:** Share-to-download target (`ACTION_SEND` text/plain, `ACTION_VIEW` http/https) and automatic clipboard detection on app resume.
- **Rust Core Engine:** Powered by `crates/swiftget-ffi` compiled via `cargo-ndk` into `jniLibs`.

---

## Architecture Overview

```
android/
├── build.gradle.kts                # Root build script (AGP 8.4.2, Kotlin 2.0.0)
├── settings.gradle.kts             # Module resolution and repositories
├── gradle/
│   ├── libs.versions.toml          # Centralized version catalog
│   └── wrapper/
│       └── gradle-wrapper.properties # Gradle 8.7 wrapper configuration
└── app/
    ├── build.gradle.kts            # Application build configuration + buildCargoNdk task
    ├── src/
    │   ├── main/
    │   │   ├── AndroidManifest.xml # Permissions, Foreground Service (dataSync), Intent filters
    │   │   ├── res/                # Vector drawables, colors, strings, themes, adaptive icons
    │   │   └── java/
    │   │       ├── com/swiftget/
    │   │       │   └── SwiftGet.kt # Direct JNI entry points matching crates/swiftget-ffi
    │   │       └── com/swiftget/app/
    │   │           ├── SwiftGetApp.kt        # Application class & DI container
    │   │           ├── MainActivity.kt       # Intent receiver & Clipboard lifecycle detection
    │   │           ├── data/
    │   │           │   ├── model/            # DownloadItem, SegmentProgress, SpeedSample, GrabbedLink
    │   │           │   └── repository/       # DownloadRepository, LinkProbeRepository
    │   │           ├── native/
    │   │           │   └── SwiftGetNative.kt # High-level typed bridge & preview fallback
    │   │           ├── service/
    │   │           │   └── DownloadService.kt# Foreground service (dataSync) with rich notifications
    │   │           └── ui/
    │   │               ├── theme/            # Material 3 Color, Type, Theme (Dynamic Color)
    │   │               ├── components/       # SegmentProgressBar, SpeedGraph, DownloadCard
    │   │               └── screens/          # Downloads, AddDownload, LinkGrabber, Settings
    │   └── test/                             # JUnit unit tests for models and bridge
```

---

## Prerequisites

To build and run the Android application:

1. **Java Development Kit (JDK):** JDK 17 or JDK 21 (e.g. OpenJDK / Temurin).
2. **Android SDK:**
   - Platform SDK: Android 14 (API 34)
   - Build Tools: 34.0.0 or later
   - NDK: 25.x or 26.x (`ndk.dir` or `ANDROID_NDK_HOME`)
3. **Rust Toolchain & cargo-ndk (for building native Rust engine):**
   ```bash
   cargo install cargo-ndk
   rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android
   ```

---

## Native Rust Engine Compilation (`cargo-ndk`)

SwiftGet uses `crates/swiftget-ffi` to compile a shared C library (`libswiftget_ffi.so`) for all target Android ABIs:
- **32-bit ARM:** `armeabi-v7a` (`armv7-linux-androideabi`)
- **64-bit ARM:** `arm64-v8a` (`aarch64-linux-android`)
- **32-bit x86:** `x86` (`i686-linux-android`)
- **64-bit x86:** `x86_64` (`x86_64-linux-android`)

### Option A: Via Gradle Task

The Android project includes a built-in Gradle task that automatically invokes `cargo-ndk` with the required parameters:

```bash
cd android
./gradlew buildCargoNdk
```

This compiles `crates/swiftget-ffi` in `--release` mode and places the resulting `.so` files directly into:
```
android/app/src/main/jniLibs/
  ├── arm64-v8a/libswiftget_ffi.so
  ├── armeabi-v7a/libswiftget_ffi.so
  ├── x86/libswiftget_ffi.so
  └── x86_64/libswiftget_ffi.so
```

### Option B: Manual Compilation

You can also run `cargo ndk` directly from the `crates/swiftget-ffi` directory:

```bash
cd crates/swiftget-ffi
cargo ndk \
  -t armeabi-v7a \
  -t arm64-v8a \
  -t x86 \
  -t x86_64 \
  -o ../../android/app/src/main/jniLibs \
  build --release
```

---

## JNI Specification & Symbol Mapping

The Kotlin native bridge connects to the Rust engine using the following JNI exports:

| Kotlin Binding Method | Exported C/Rust JNI Symbol | Arguments | Return Type | Description |
|:---|:---|:---|:---|:---|
| `SwiftGet.nativeStart` | `Java_com_swiftget_SwiftGet_nativeStart` | `String url, String output, int connections` | `jlong` | Starts download task; returns handle ID (>0) |
| `SwiftGet.nativePause` | `Java_com_swiftget_SwiftGet_nativePause` | `long id` | `jboolean` | Pauses active download |
| `SwiftGet.nativeResume` | `Java_com_swiftget_SwiftGet_nativeResume` | `long id` | `jboolean` | Resumes paused download |
| `SwiftGet.nativeCancel` | `Java_com_swiftget_SwiftGet_nativeCancel` | `long id` | `jboolean` | Cancels download and frees handle |
| `SwiftGet.nativeStatus` | `Java_com_swiftget_SwiftGet_nativeStatus` | `long id` | `jstring` | Returns real-time JSON status payload |

### JSON Status Format

`nativeStatus(id)` outputs a JSON object:
```json
{
  "id": 1,
  "state": "running",
  "downloaded": 104857600,
  "total": 524288000,
  "speed_bps": 15728640.0,
  "progress_pct": 20.0
}
```

*Note:* If the application is compiled or run without `libswiftget_ffi.so` present in `jniLibs` (e.g. during rapid UI prototyping in Android Studio preview), `SwiftGetNative` automatically falls back to an integrated mock simulation engine so the UI remains fully functional.

---

## Building the Android APK / App Bundle

### Debug APK
```bash
cd android
./gradlew assembleDebug
```
Output: `android/app/build/outputs/apk/debug/app-debug.apk`

### Install on Connected Device or Emulator
```bash
./gradlew installDebug
```

### Release Android App Bundle (AAB for Google Play)
```bash
./gradlew bundleRelease
```
Output: `android/app/build/outputs/bundle/release/app-release.aab`

### Running Unit Tests
```bash
./gradlew testDebugUnitTest
```

---

## System Integration & Features

### 1. Foreground Service (`dataSync`)
Declared in `AndroidManifest.xml`:
```xml
<service
    android:name=".service.DownloadService"
    android:enabled="true"
    android:exported="false"
    android:foregroundServiceType="dataSync" />
```
Provides persistent execution for large downloads when the app is in the background, updating notification progress and handling `Pause All` / `Resume All` actions directly from the notification tray.

### 2. Share-to-Download Intent Filters
Accepts links shared from web browsers, YouTube, Twitter, and other apps:
- `ACTION_SEND` with `text/plain`
- `ACTION_VIEW` with `http` and `https` schemes

### 3. Clipboard Auto-Capture
When enabled in Settings, `MainActivity` inspects the clipboard during `onResume()`. When a valid HTTP/HTTPS link is detected, an interactive banner appears at the top of the UI offering a one-click download action.

### 4. Dynamic Color (Material You)
On Android 12+ (API 31+), the UI adapts to the user's wallpaper palette using `dynamicLightColorScheme` and `dynamicDarkColorScheme`, with full manual dark mode overrides.
