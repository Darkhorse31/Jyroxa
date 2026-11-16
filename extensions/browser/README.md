# Jyroxa browser extension

Chromium (Chrome, Edge) and Firefox WebExtension that intercepts downloads and forwards them to the Jyroxa desktop app via the native messaging host (`jyroxa-native-host`).

The shipped manifest includes a stable `key`; the Chromium extension ID is **`njgbdpacpmohafoefmcmnmhdjpjmjipe`** (private key is kept outside the repo).

## Load unpacked (development)

1. Build `jyroxa-native-host` and `jyroxa-desktop`; keep `jyroxa-native-host.exe` next to `jyroxa-desktop.exe`.
2. Start Jyroxa once — the native host is registered automatically (Chrome, Edge, Firefox, HKCU).
3. Chrome: `chrome://extensions` → Developer mode → Load unpacked → this folder.
4. Edge: `edge://extensions` → Load unpacked.
5. Firefox: `about:debugging` → This Firefox → Load Temporary Add-on → `manifest.json`.

Hold **Alt** while clicking a link to skip interception for ~30 seconds.