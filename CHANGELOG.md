# Changelog

## 0.1.4 — 2026-11-13

### Desktop UI

- New square, flat design (design/v2): denser task list, slim navigation, thin progress lines, and refined light/dark themes.
- Settings reorganized into tabs with consistent cards and thin borders.

### Browser integration

- WebExtension for Chrome, Edge, and Firefox intercepts downloads and forwards them to the desktop app via native messaging.
- Settings → Browser tab: enable integration, file-type filters, extension IDs, and one-click native host install.
- Release bundles `jyroxa-native-host.exe` beside the desktop app and in the portable ZIP; `dist/jyroxa-browser-extension.zip` ships the unpacked extension.

### Engine (unchanged from 0.1.3 speed work)

- Adaptive auto mode, benchmark showcase in README, and v0.1.3 throughput improvements remain on this branch.

### Distribution

- Workspace, desktop, and Android version 0.1.4; Android versionCode 5.
- Windows NSIS installer, portable ZIP (with native host), browser extension ZIP, signed Android universal APK, and SHA-256 checksums.

## 0.1.3 — 2026-11-13

### Speed

- Auto mode starts range downloads with eight connections (up to the configured maximum), tests wider concurrency, and reverts trials that do not improve aggregate throughput.
- Idle connections split active tails safely; buffered writes preserve committed cursors and checksums.
- Removed redundant metadata requests and unlimited-transfer rate-limiter overhead; successful host measurements persist after writes complete.
- Added a reproducible benchmark showcase, raw release confirmation output, and a comparison chart with clearly labeled historical IDM 6.42 references. See [docs/BENCHMARK.md](docs/BENCHMARK.md) for measured results and limits.

### Desktop and LinkGrabber

- Manual connection counts open the requested connections immediately; the segment map stays current.
- Improved responsive search and toolbar layout, LinkGrabber rows and file filtering, including ignoring URLs inside tags and scripts.

### Distribution

- Workspace, desktop, and Android version 0.1.3; Android versionCode 4.
- Windows NSIS installer, portable ZIP, signed Android universal APK, and SHA-256 checksums.

## 0.1.2 — 2026-11-12

- Windows restores saved windows onto the current screen and brings existing instances into view; startup crash details are logged.
- Android uses unique download IDs, removes sample downloads, cleans damaged saved lists, and resumes saved downloads through the engine.
- Kept the existing Android signing identity for upgrades; Android versionCode 3.

## 0.1.1 — 2026-11-11

### Product

- Renamed the project from SwiftGet to **Jyroxa** across crates, CLI (`jyroxa`), desktop (Tauri), and Android (`com.jyroxa.app`).
- State files now use `.jxstate` with backward-compatible `.sgstate` resume.
- App data and bench directories use `.jyroxa` / `.jyroxa-bench` (legacy `.swiftget` paths still read where applicable).

### Engine

- Stall recovery, bounded segment stealing, and optional engine trace (`JYROXA_TRACE` / `SWIFTGET_TRACE`).
- Adaptive tuner guarantees throughput does not fall below single-connection baseline when auto-tuning.

### Desktop

- Single instance, close-to-tray, native completion/failure notifications, Jyroxa branding and icons.
- QA fix: avoid duplicate default folder when saving settings.

### Android

- JNI via `libjyroxa_ffi.so` (arm64-v8a, armeabi-v7a, x86_64), `versionCode` 2, release APK `Jyroxa-0.1.1-universal.apk`.

### Docs & hygiene

- README, build docs, and repository `.gitignore` updated for open-source release under MIT (Copyright Prateek Kumar).
