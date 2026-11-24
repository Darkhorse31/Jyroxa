# Jyroxa browser extension

Chromium (Chrome, Edge) and Firefox WebExtension that intercepts downloads and forwards them to the Jyroxa desktop app via the native messaging host (`jyroxa-native-host`).

The shipped manifest includes a stable `key`; the Chromium extension ID is **`njgbdpacpmohafoefmcmnmhdjpjmjipe`** (private key is kept outside the repo).

## Load unpacked (development)

1. Build `jyroxa-native-host` and `jyroxa-desktop`; keep `jyroxa-native-host.exe` next to `jyroxa-desktop.exe`.
2. Start Jyroxa once — the native host is registered automatically for Chrome, Edge, Brave, Chromium and Firefox in HKCU. The installer also registers it without opening the app.
3. Chrome: `chrome://extensions` → Developer mode → Load unpacked → this folder.
4. Edge: `edge://extensions` → Load unpacked.
5. Firefox: `about:debugging` → This Firefox → Load Temporary Add-on → `manifest.json`.

Hold **Alt** while clicking a link to skip interception for ~30 seconds.

## Headless verification

Run `npm test` here for the extension unit tests. Build the workspace release binaries and the desktop `release-gate` feature, prepare video sidecars with `scripts/fetch-video-sidecars.ps1`, then run `npm run test:integration`. Registration tests use temporary data and registry prefixes. Live video tests use the windowless engine with an isolated named pipe; they never start the desktop GUI or write into the user's Downloads folder.

The Windows video bundle includes Node (with its license), yt-dlp (with EJS), and ffmpeg. Add Download accepts YouTube watch, short and Shorts URLs, shows qualities, and offers optional browser cookies plus Retry when YouTube asks for sign-in. Without sign-in requirements, browser cookies are unnecessary. Chromium cookie encryption may prevent yt-dlp from reading a browser profile; sending a link from the extension supplies its cookies directly.
