# Headless Windows release verification

Use the existing development environment and the worktree's `.toolbin` on PATH.
Do not run the desktop executable for headless checks: `--minimized` still creates
a webview. The opt-in harness uses the real desktop engine with an empty Tauri
window configuration. It creates no tray, installs no browser integration, and
disables notifications. Normal builds exclude the harness.

```powershell
. "$env:USERPROFILE\.jyroxa-tools\env.ps1"
$env:PATH = "$PWD\.toolbin;" + $env:PATH
cargo test --workspace --release -j 3 -- --test-threads=1
cargo clippy --workspace --all-targets --features jyroxa-desktop/release-gate -j 3 -- -D warnings
$env:JYROXA_TORRENT_LIVE = '1'
cargo test -p jyroxa-core --test torrent_live --release -j 3 -- --nocapture
Remove-Item Env:JYROXA_TORRENT_LIVE
$releaseRoot = Join-Path $(if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { "$PWD\target" }) 'release'
$stamp = Get-Date -Format 'yyyyMMddHHmmss'
$env:JYROXA_VERIFIED_SIDECARS_DIR = Join-Path $env:TEMP "jyroxa-verified-sidecars-$stamp"
New-Item -ItemType Directory $env:JYROXA_VERIFIED_SIDECARS_DIR | Out-Null
foreach ($binary in @(
    @('jyroxa-cli', 'jyroxa'),
    @('jyroxa-native-host', 'jyroxa-native-host'),
    @('jyroxa-native-host', 'jyroxa-browser-register')
)) {
    cargo rustc --release -p $binary[0] --bin $binary[1] -j 3 -- "-Cmetadata=verified_$stamp"
    if ($LASTEXITCODE -ne 0) { throw 'Sidecar build failed' }
    Copy-Item (Join-Path $releaseRoot ($binary[1] + '.exe')) $env:JYROXA_VERIFIED_SIDECARS_DIR
}
foreach ($name in @('WebView2Loader.dll', 'yt-dlp.exe', 'ffmpeg.exe')) {
    Copy-Item "apps/desktop/src-tauri/$name" $env:JYROXA_VERIFIED_SIDECARS_DIR
}
& apps/desktop/prepare-bundle-resources.ps1
cargo build --release -p jyroxa-desktop --features release-gate --bin jyroxa-release-gate -j 3
if ($LASTEXITCODE -ne 0) { throw 'Harness build failed' }
node scripts/release-gate.mjs
```

The live test discovers the current Debian netinst torrent from the official
directory, independently parses its info hash, then supplies only that hash to a
fresh DHT session with trackers disabled. Network tests are opt-in; a default
test run does not establish live DHT success.

Serial libtest scheduling isolates the throughput measurements from unrelated
tests. The unlimited-server regression still includes all 64 balanced timed
rounds per mode, validates every payload, and requires at least 97% of manual
throughput. Its threshold and the separate capped-server requirement are unchanged.

The session script copies the release harness, native host, CLI, WebView2 loader,
yt-dlp and ffmpeg into a unique temporary directory. It checks a framed native
host ping, HTTP pause/resume across a process restart, three distinct official
torrents (file URL, HTTPS, tracker-free magnet), deduplication, torrent pause and
sidecar termination/retry, plus YouTube and Dailymotion qualities and downloads.
Torrent start deadlines remain ten seconds and completed payloads must match
Debian's SHA-256 sums. Failures stop the gate and preserve `results.json`, queue
data, and process logs under the printed evidence directory. Public torrent
checks can download several gigabytes; remove only those disposable evidence
payloads after recording results.

Keep the verified sidecars outside Cargo's output directory. Tauri copies bundle
resources into that directory during a desktop build and can overwrite CLI/native
host executables with older resource copies. The unique compiler metadata above
forces fresh binaries even if Cargo still considers a replaced output cached.
Use the same `JYROXA_VERIFIED_SIDECARS_DIR` for the comparison, resource preparation,
and portable packaging; the gate records the hashes of the binaries it actually runs.

Run the headless extension fixtures and the comparative HTTP benchmark separately.
The comparison script warms two output files, balances each binary across both
files and run positions, and uses fresh host memory for every invocation. Its
default eight timed runs per version include every sample, verify all payload
hashes, and require both current medians to meet or exceed the published baseline.
All checks must pass before packaging, merging, tagging, or publishing a release.
