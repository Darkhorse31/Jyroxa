# Copy release binaries beside jyroxa-desktop for Tauri NSIS resources.
$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$targetRoot = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $repoRoot "target" }
if (-not [System.IO.Path]::IsPathRooted($targetRoot)) { $targetRoot = Join-Path $repoRoot $targetRoot }
$target = if ($env:JYROXA_VERIFIED_SIDECARS_DIR) { $env:JYROXA_VERIFIED_SIDECARS_DIR } else { Join-Path $targetRoot "release" }
$dest = Join-Path $PSScriptRoot "src-tauri"

$map = @(
    @{ Name = "jyroxa-native-host.exe" },
    @{ Name = "jyroxa-browser-register.exe" },
    @{ Name = "jyroxa.exe" },
    @{ Name = "WebView2Loader.dll" },
    @{ Name = "yt-dlp.exe" },
    @{ Name = "ffmpeg.exe" }
)

foreach ($item in $map) {
    $src = Join-Path $target $item.Name
    if (-not (Test-Path $src)) {
        throw "Missing required bundle resource $src — build and stage the release sidecars first."
    }
    Copy-Item $src (Join-Path $dest $item.Name) -Force
    Write-Host "Staged $($item.Name) for bundle"
}
