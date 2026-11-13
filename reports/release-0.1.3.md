# Release v0.1.3 — 2026-11-13

Repository: `C:\Users\lenovo\Desktop\Jyroxa`, branch `main`, starting revision `cddd95a923572a1753a6c02f480ae6b3218eeb68`. Dot-sourced `C:\Users\lenovo\.jyroxa-tools\env.ps1` before commands. All builds and checks were headless; no visible app window, virtual desktop switching, or IDM launch was used. Neither `feature/telegram` nor `design/v2` was merged.

## Tests and measurements

- `cargo test -p jyroxa-core`: **PASS**, 67 unit tests, 20 integration tests, 0 doc tests; zero failures. This ran against the starting engine source; subsequent Rust changes only bump workspace/lockfile versions.
- `cargo build --release -p jyroxa-cli`: **PASS**, exit 0, version 0.1.3. Benchmark executable SHA-256: `C71B74615AFF28B6A062DF592FC51ECCC75FA4E3C99C19ED7615D803DF1AD57D`.
- Both requested benchmark commands: **PASS**, exit 0; three auto repetitions per server, 30-second cooldowns, with a further 30 seconds between server commands. Auto measurements ran before packaging builds.

| Server | v0.1.2 auto MB/s | v0.1.3 auto min / median / max MB/s | IDM 6.42 reference MiB/s → MB/s | Auto / IDM, matching units |
| --- | ---: | ---: | ---: | ---: |
| Singapore | 5.52 | 21.92 / **24.59** / 27.96 | 10.08 → 10.57 | **2.33×** |
| Falkenstein | 5.36 | 8.70 / **10.13** / 10.89 | 6.72 → 7.05 | **1.44×** |

The same PC and Hetzner `100MB.bin` files were used for the archived references (104,857,600 bytes each). Historical v0.1.2 and IDM medians were measured earlier on 2026-11-13 with three runs and ten-second cooldowns. IDM's supplied 10.08 / 6.72 values use MiB/s, as verified by the archived byte counts and timings. Speedups divide the new decimal MB/s values by the historical references multiplied by 1.048576. They are approximate and do not establish a universal ranking.

The earlier auto results **21.60 / 10.37 MB/s** remain in [beat-idm-2026-11-13.md](beat-idm-2026-11-13.md); the showcase uses the fresh confirmation above. [Reference provenance](benchmark-reference-2026-11-13.md) and [original IDM timing rows](idm-reference-2026-11-13.csv) explain the unit correction.

[Raw confirmation output](release-0.1.3-bench.log) retains both commands, timestamps, auto min/median/max, and extra fixed-16 controls. This CLI schedules fixed 16 even with `--modes auto`; its control medians were 1.49 / 1.45 MB/s and are excluded from the showcase. The final Falkenstein control overlapped packaging work. Mixed PowerShell UTF-8/UTF-16 logging was normalized to UTF-8 without changing output values.

Ghost Downloader **could not be benchmarked headlessly yet**; no Ghost measurements are claimed.

## Builds and artifact checks

The v0.1.2 GitHub release inventory was inspected before building: Windows NSIS installer, portable ZIP with `Jyroxa.exe` and `WebView2Loader.dll`, signed three-ABI Android universal APK, and SHA-256 sums. v0.1.3 uses the same Tauri/GNU, Gradle/cargo-ndk, signing, and packaging formats.

- Windows Tauri `--ci` build: **PASS**, exit 0; frontend contract check (20 commands, 19 invoke sites, 12 TypeScript files), TypeScript, Vite, optimized native build, and NSIS bundling succeeded. [Build log](release-0.1.3-windows-build.log).
- Windows executable metadata: **PASS**, FileVersion and ProductVersion both `0.1.3`. Portable ZIP contents: exactly `Jyroxa.exe` (14,990,083 bytes) and `WebView2Loader.dll` (160,880 bytes).
- Android `assembleRelease` and `exportReleaseApk`: **PASS**, exit 0; all three cargo-ndk ABI builds, Kotlin/Java compilation, release lint and signed APK packaging succeeded. [Build log](release-0.1.3-android-build.log).
- APK verification: **PASS**, signature scheme v2; package `com.jyroxa.app`, versionName `0.1.3`, versionCode `4`, minSdk 24, targetSdk 34. APK includes `libjyroxa_ffi.so` for `arm64-v8a`, `armeabi-v7a`, and `x86_64`. [Signature and manifest inspection](release-0.1.3-apk-verification.log).
- APK signing certificate SHA-256: `0f745a8e88a42fe60df51bb530c5672ca54b66496ea0d9283973a8af77cf0245`, identical to the verified v0.1.2 release APK. Exported and inspected APK hashes match.
- SHA-256 sums generated for the installer, portable ZIP and APK. SVG parsed as XML; chart values match the raw auto medians and normalized IDM references. `git diff --check`: PASS before commit.

The existing desktop `build-gnu.ps1` contains legacy `E:` paths. The already-configured environment supplies the repo `.toolbin` and NDK tools, so the equivalent bundler command was run directly from `apps/desktop`:

```powershell
npm run tauri build -- --ci
```

Android command, from `android`:

```powershell
.\gradlew.bat --no-daemon --console=plain :app:assembleRelease :app:exportReleaseApk
```

The portable ZIP uses the same two files as v0.1.2. Gradle's exported `Jyroxa-0.1.3-universal.apk` is staged as `Jyroxa-0.1.3-android-universal.apk` to match the previous release's naming convention. Distribution artifacts live under git-ignored `dist/`.

## Artifact inventory

Local artifacts prepared for upload; publication verification follows below.

| Artifact | Size (bytes) | SHA-256 |
| --- | ---: | --- |
| `Jyroxa_0.1.3_x64-setup.exe` | 3824605 | `c0e954394ead36b2e961bb926ccbf2e25e77a096f834b911fa357e29aa4cd0ba` |
| `Jyroxa-0.1.3-android-universal.apk` | 26911412 | `1476b4af0be36d9183d38f0a8c02d5a3166d9227b00898bf4564e97946f7ae93` |
| `Jyroxa-0.1.3-windows-x64-portable.zip` | 5244496 | `61387f748a5d7c2880ce1915784c95b922a231190f727a57289a186ff84df9b5` |
| `SHA256SUMS.txt` | 301 | `277324bb9a8833651c4e3e3c998c4a1ca9e0b70ba4ee4e32f8162ad40850b7ec` |

## Publication

Release target: https://github.com/Darkhorse31/Jyroxa/releases/tag/v0.1.3. Builds and local artifacts verified; publication is pending at this source commit. The final uploaded-asset verification is recorded in the report on `main` after publication.

## Failures and limitations

No test or build failed. Publication verification is pending at this source commit.

- The Android build reports a pre-existing deprecated `android.defaults.buildfeatures.buildconfig=true` option.
- Android's `stripReleaseDebugSymbols` could not strip `libjyroxa_ffi.so`; Gradle packaged the libraries as-is and completed release compilation, lint, signing and export successfully.
- Tauri warns that the existing `com.jyroxa.app` identifier ends in `.app`, a macOS bundle naming concern; the Windows NSIS build succeeded.
- The initial CLI build log contains PowerShell `NativeCommandError` formatting for Cargo's normal stderr progress output; Cargo itself completed successfully with exit 0.
- Extra fixed-16 benchmark scheduling and the historical IDM unit mismatch are documented above; no measurements were invented or substituted.
- Runtime GUI smoke tests were not performed under the headless-only instruction. Artifact checks inspect versions, package contents and signatures without launching apps.
