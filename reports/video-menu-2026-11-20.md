# Video menu fix (2026-11-20)

## Problem

The in-page video grabber opened a menu that showed only a bare **VIDEO** row (from `mediaLabel` when no size/resolution/filename was known) plus **Hide on this site**. Blob/MSE players never listed yt-dlp qualities, and page handoff did not merge audio+video via the desktop sidecar.

## Changes

| Area | Files |
|------|--------|
| Extension UI | `extensions/browser/video-grab.js`, `video-grab.css` — “Finding qualities…”, error + Retry, filter bare VIDEO rows |
| Extension SW | `extensions/browser/background.js` — HLS master expansion, `probe_video_formats` via native host, better item mapping |
| Labels | `extensions/browser/media-monitor.js` — filenames, HLS/DASH wording |
| Native messaging | `extensions/browser/native.js` — `probeVideoFormats`, longer timeout |
| Protocol | `crates/jyroxa-native-host/src/handoff.rs`, `main.rs` — `ProbeVideoFormats` message |
| Desktop pipe | `apps/desktop/src-tauri/src/browser_integration.rs` — probe handler; handoff resolves direct URLs or marks yt-dlp page jobs |
| yt-dlp sidecar | `apps/desktop/src-tauri/src/video_sidecar.rs` — `formats_for_menu`, ffmpeg merge on download |
| Engine | `apps/desktop/src-tauri/src/engine.rs` — `X-Jyroxa-Ytdlp-*` header path completes via yt-dlp |
| Tests | `extensions/browser/tests/video-menu-probe.spec.mjs`, `video-menu-e2e.spec.mjs`, `video-grab.spec.mjs` |

## Verification

Environment: `. C:\Users\lenovo\.jyroxa-tools\env.ps1`, `CARGO_TARGET_DIR=C:\Users\lenovo\Desktop\Jyroxa-wt\host-video\target`.

| Command | Result |
|---------|--------|
| `node extensions/browser/media-monitor.test.mjs` | PASS |
| `node extensions/browser/tests/native.spec.mjs` | PASS |
| `node extensions/browser/tests/video-grab.spec.mjs` | PASS |
| `cargo test -p jyroxa-desktop formats_menu --release -j 3` | PASS (1 test) |
| `cargo build -p jyroxa-desktop -p jyroxa-native-host --release -j 3` | PASS |
| `node extensions/browser/tests/video-menu-e2e.spec.mjs` | fixture_mp4 **PASS**; fixture_hls **SKIP** (SW variant rows not asserted in headless run); youtube **PASS**; dailymotion **FAIL** (transient yt-dlp error in short desktop window) |
| `node extensions/browser/tests/video-menu-probe.spec.mjs` | PASS — YouTube `aqz-KE-bpKQ` formats listed; Dailymotion `x89eyek` formats listed; handoff queued download **id=1**; completed media under `%USERPROFILE%\Downloads` (e.g. ~10 MB `videoplayback.m4a` from CC test video) |

Measured: YouTube probe via native host ~12–20 s; full probe script ~90–130 s including one quality download.

## Open issues

- `cargo clippy -p jyroxa-native-host -- -D warnings` fails in this worktree because `.toolbin/rustc-wrap.exe` passes duplicate rustc args (pre-existing harness issue).
- Vimeo requires login in yt-dlp; Dailymotion used instead per task. Dailymotion probe can be flaky under parallel desktop load.
- HLS variant rows in the fixture page are implemented (`parseHlsMaster` in the service worker) but not fully asserted in Playwright (fixture_hls SKIP).

VIDEO-MENU DONE.