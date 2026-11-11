# Changelog

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
