# SwiftGet build outputs

## Android (release APK)

Built on **2026-11-10** with JDK 17, Android SDK 35 (platform 34 + build-tools 34.0.0), NDK 27.2, Gradle 8.7.

| Artifact | Path |
|----------|------|
| Release APK (arm32 + arm64, signed) | `E:\SwiftGet\dist\SwiftGet-0.1.0-universal.apk` |
| Gradle output (same bytes) | `E:\SwiftGet\android\app\build\outputs\apk\release\app-release.apk` |

### Commands used (isolated toolchain env)

```powershell
$env:JAVA_HOME="E:\.swiftget-tools\jdk17"
$env:ANDROID_HOME="E:\.swiftget-tools\android-sdk"
$env:ANDROID_NDK_HOME="E:\.swiftget-tools\android-sdk\ndk\27.2.12479018"
$env:ANDROID_USER_HOME="E:\.swiftget-tools\android-user-home"
$env:GRADLE_USER_HOME="E:\.swiftget-tools\gradle-home"
$env:RUSTC_WRAPPER=""   # skip Windows GNU rustc wrapper when cross-compiling

cd E:\SwiftGet\crates\jyroxa-ffi
cargo ndk -t arm64-v8a -t armeabi-v7a -o E:\SwiftGet\android\app\src\main\jniLibs build --release

cd E:\SwiftGet\android
.\gradlew.bat assembleRelease
```

Rust on Android uses **rustls** (via `jyroxa-core` target cfg); Windows desktop builds still use **native-tls** (schannel).

---

## Desktop (Windows GNU)

Built on **2026-11-11** with host `x86_64-pc-windows-gnu`, Tauri v2.

| Artifact | Path |
|----------|------|
| Release executable | `E:\SwiftGet\target\release\jyroxa-desktop.exe` |
| WebView2 loader (required beside exe on GNU) | `E:\SwiftGet\target\release\WebView2Loader.dll` |
| NSIS installer | `E:\SwiftGet\dist\SwiftGet_0.1.0_x64-setup.exe` |
| Portable zip (`SwiftGet.exe` + `WebView2Loader.dll`) | `E:\SwiftGet\dist\SwiftGet-0.1.0-windows-x64-portable.zip` |
| Frontend bundle (embedded) | `E:\SwiftGet\apps\desktop\dist\` |

### Commands used

```powershell
cd E:\SwiftGet\apps\desktop
npm install
powershell -NoProfile -ExecutionPolicy Bypass -Command "cd E:\SwiftGet\apps\desktop; .\build-gnu.ps1"
.\package-portable.ps1
Copy-Item E:\SwiftGet\target\release\bundle\nsis\SwiftGet_0.1.0_x64-setup.exe E:\SwiftGet\dist\ -Force
```

`build-gnu.ps1` prepends `.toolbin` (windres/ar shims) and the NDK LLVM bin directory to `PATH` so `tauri-winres` can compile the Windows manifest.

**Note:** A lone copy of `jyroxa-desktop.exe` (or a zip containing only the exe) will fail at startup with a missing `WebView2Loader.dll` error on GNU builds. Use the portable zip or the NSIS installer.

## Workspace Rust binaries (reference)

| Artifact | Path |
|----------|------|
| CLI (if built) | `E:\SwiftGet\target\release\swiftget.exe` |
