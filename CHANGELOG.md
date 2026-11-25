## 0.1.15 — 2026-11-25

### YouTube download reliability

- Bundle yt-dlp 2026.11.23 with a pinned SHA-256 and verify both before packaging, so an old sidecar cannot survive a new build.
- Detect the current `addOnDisposeCallback` YouTube player error and explain that an app update is needed.
- Add the reported YouTube video to the headless release gate for metadata, quality, download, and decode verification.

### Distribution

- Workspace, desktop, extension, and Android 0.1.15; Android versionCode 15.

## 0.1.14 — 2026-11-25

### YouTube downloads and browser extension

- Show live YouTube transfer progress in the task card, including downloaded and remaining bytes, percent, speed, remaining time, and merging/processing states.
- Keep the in-page video quality menu above site controls and within the viewport, with stable hover behavior.

### Distribution

- Workspace, desktop, extension, and Android 0.1.14; Android versionCode 14.

## 0.1.13 — 2026-11-24

### YouTube downloads

- Route YouTube watch, short, Shorts, live, and embed URLs through yt-dlp from Add Download and Link Grabber, with canonical URL handling, metadata, format choices, and complete MP4 downloads.
- Use an available JavaScript runtime for YouTube extraction, surface actionable extraction errors with Retry, and support optional browser cookies when YouTube requires sign-in.

### Browser extension

- Register valid, browser-specific native messaging manifests for Chrome, Edge, Brave, Chromium, and Firefox, including discovered installed extension IDs.
- Distinguish unavailable, forbidden, and failed native host connections without the misleading “Host not registered” message.

### Distribution

- Workspace, desktop, extension, and Android 0.1.13; Android versionCode 13.

# Changelog

## 0.1.12 — 2026-11-23

### Browser native host

- Register native messaging for Chrome, Edge, Brave, and Firefox on every app start and install.
- Native host manifest keeps the packaged extension ID plus legacy unpacked IDs; copies `jyroxa-native-host.exe` into `%LOCALAPPDATA%\\Jyroxa` for a stable manifest path.
- Extension popup shows the real connection error, auto-retry, extension ID, and reload-Chrome guidance; error text no longer mis-labels pipe errors as “Host not registered”.
- Native host survives panics per message and returns JSON errors instead of exiting.

### HTTP and torrent reliability

- Reuse saturated-host connection counts and bound worker growth to the selected connection cap.
- Claim the fast-start lead range once so parallel workers do not download duplicate bytes.
- Resolve torrent metadata without unconditional bootstrap delays, with progress and timeout coverage during magnet resolution.
- Preserve torrent output folders and per-source session state across pause, resume, and restart.
- Release the active-download lock before pumping the torrent queue so Retry can recover after a sidecar exits.
- Accept native-host magnet and local torrent handoffs, including file URLs containing literal plus signs.

### Windows sidecars

- Keep torrent and video subprocesses hidden and terminate them with the desktop app.
- Use a browser client for Dailymotion format probes while preserving full-download throughput with the default media client.
- Refresh Dailymotion metadata after CDN rejection and fail incomplete video fragments instead of silently skipping them.
- Prefer portable `sidecars` binaries when the desktop executable is named `Jyroxa.exe`.
- Respect the notification setting for completed and failed downloads.

### Distribution

- Workspace, desktop, extension, and Android 0.1.12; Android versionCode 12.

## 0.1.11 — 2026-11-19

### BitTorrent reliability

- Metadata phase emits progress (`phase=metadata`, peer counts) instead of a silent 0 B/s stall; DHT/trackers use the built-in list and a 300s metadata timeout with a clear UI message.
- Download stall detection (120s without progress) with firewall/peer hints; sidecar stderr errors and missing `jyroxa.exe` are surfaced in the task card.
- Sidecar stdout is drained so pipes cannot block; desktop passes peer limit / tracker / seeding settings to `jyroxa torrent get`; queue restore picks up partial torrent bytes on disk.

### Browser video grab (IDM-style)

- Floating teal download control on `<video>` / `<audio>` elements (MutationObserver, all frames).
- Captures direct media, HLS `.m3u8`, and DASH `.mpd` via `webRequest`; MSE/blob pages hand off the page URL for yt-dlp.
- Popup adds a global video-button toggle; per-site hide; connection auto-retry before errors (reload extension + restart Chrome after install/update).

### Engine

- Parallel HLS/DASH segment download and merge/remux to MP4 in `jyroxa-core`.
- Windows bundles `yt-dlp.exe` and `ffmpeg.exe` beside the desktop app (NSIS + portable).

### Distribution

- Workspace, desktop, extension, and Android 0.1.11; Android versionCode 11.

## 0.1.10 — 2026-11-19

### BitTorrent / magnets

- Single-instance pipe now emits `external-url` for `magnet:` and `.torrent` handoffs, not only HTTP(S).
- Launch argv parsing and dedupe helpers; extension intercepts magnet link clicks via native host.
- Restores and focuses the main window when a second instance forwards a torrent target.

### Distribution

- Workspace, desktop, extension, and Android 0.1.10; Android versionCode 10.

## 0.1.9 — 2026-11-19

### Telegram

- Public channel posts whose embed widget says “Please open Telegram to view this post” resolve through the signed-in MTProto session when available.
- Add-dialog probe shows Settings guidance when the file is embed-hidden and no Telegram account is connected.

### Distribution

- Workspace, desktop, extension, and Android 0.1.9; Android versionCode 9.

## 0.1.8 — 2026-11-16

### BitTorrent

- librqbit-based torrent engine in the CLI sidecar (`jyroxa.exe`); desktop avoids linking librqbit directly.
- Magnet and `.torrent` URLs, file associations (HKCU), single-instance handoff, and browser extension handoff for magnets and `.torrent` downloads.
- Add dialog resolves torrent metadata (name, size, multi-file checkboxes) before starting.
- NSIS bundle ships `jyroxa.exe` beside `jyroxa-desktop.exe` for the sidecar.

### Benchmark

- Debian 13.7.0 netinst torrent: Jyroxa ~50 MiB/s vs tuned aria2c ~15 MiB/s on this host (see README).

**Not included:** BEP-19 web seeds through the HTTP multi-connection engine.

### Distribution

- Workspace, desktop, extension, and Android 0.1.8; Android versionCode 8.

## 0.1.7 — 2026-11-16

### Browser handoff

- Restores and focuses the desktop window on browser handoff and link grabber events (tray/minimized no longer hides the Add flow).
- Add dialog probes handoff URLs with Cookie/Referer/User-Agent so extensionless links get real filenames and sizes.
- Extension dedupes `onCreated` and `onDeterminingFilename`, cancels Chrome only after a successful handoff, and notifies when Jyroxa is unreachable.
- Extension popup toggle **Start downloads without asking** syncs with desktop `silent_add`; silent handoffs show a native **Added to Jyroxa** notification.

### Distribution

- Workspace and browser extension 0.1.7; NSIS installer and extension ZIP.

## 0.1.6 — 2026-11-16

### Browser integration

- Native messaging host registers automatically on every app start and from the Windows installer (Chrome, Edge, Firefox).
- Stable shipped Chromium extension ID (`njgbdpacpmohafoefmcmnmhdjpjmjipe`); auto-registration keeps discovered unpacked extension IDs.
- Extension popup shows clearer errors when the host is missing or unreachable.
- Tests isolate HKCU registry writes via `JYROXA_REGISTRY_PREFIX` so developer machines are not affected.

### Distribution

- Workspace, desktop, and Android version 0.1.6; Android versionCode 7.

## 0.1.5 — 2026-11-16

### Telegram downloads

- Public `t.me/channel/id` posts resolve via embed HTML with no login; private `t.me/c/…` links use optional MTProto sign-in (Settings → Telegram).
- LinkGrabber and add-download flows recognize Telegram post URLs alongside regular HTTP links.

### Desktop build

- Windows GNU: desktop `crate-type` is `rlib` + `staticlib` only (no `cdylib`) so grammers does not blow the DLL export ordinal limit.

### Distribution

- Workspace, desktop, and Android version 0.1.5; Android versionCode 6.
- Same artifact set as 0.1.4 (NSIS, portable ZIP with native host, browser extension ZIP, Android APK, SHA-256 checksums). Android APK does not include Telegram (desktop-only feature).

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
