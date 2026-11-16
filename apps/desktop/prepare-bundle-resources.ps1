# Copy release binaries beside jyroxa-desktop for Tauri NSIS resources.
$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$target = Join-Path $repoRoot "target\release"
$dest = Join-Path $PSScriptRoot "src-tauri"

$map = @(
    @{ Name = "jyroxa-native-host.exe" },
    @{ Name = "jyroxa-browser-register.exe" },
    @{ Name = "jyroxa.exe" }
)

foreach ($item in $map) {
    $src = Join-Path $target $item.Name
    if (-not (Test-Path $src)) {
        Write-Warning "Missing $src — run cargo build --release from repo root first."
        continue
    }
    Copy-Item $src (Join-Path $dest $item.Name) -Force
    Write-Host "Staged $($item.Name) for bundle"
}