# Jyroxa browser extension

Chromium (Chrome, Edge) and Firefox WebExtension that intercepts downloads and forwards them to the Jyroxa desktop app via the native messaging host (`jyroxa-native-host`).

## Load unpacked (development)

1. Build `jyroxa-native-host` and place `jyroxa-native-host.exe` next to `jyroxa-desktop.exe`.
2. In Jyroxa **Settings → Browser**, enter your Chromium extension ID and click **Install / update host**.
3. Chrome: `chrome://extensions` → Developer mode → Load unpacked → select this folder.
4. Edge: `edge://extensions` → Load unpacked.
5. Firefox: `about:debugging` → This Firefox → Load Temporary Add-on → `manifest.json`.

Hold **Alt** while clicking a link to skip interception for ~30 seconds.
