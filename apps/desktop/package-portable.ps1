param([string]$OutputDir)
# Package a portable Windows x64 zip: Jyroxa.exe + WebView2Loader.dll (GNU/Tauri).
$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$tauriConf = Join-Path $PSScriptRoot "src-tauri\tauri.conf.json"
$conf = Get-Content $tauriConf -Raw | ConvertFrom-Json
$version = $conf.version
$targetRoot = if ($env:CARGO_TARGET_DIR) { [IO.Path]::GetFullPath($env:CARGO_TARGET_DIR) } else { Join-Path $repoRoot "target" }
$targetDir = Join-Path $targetRoot "release"
$distDir = if ($OutputDir) { [IO.Path]::GetFullPath($OutputDir) } else { Join-Path $repoRoot "dist" }
$exeSrc = Join-Path $targetDir "jyroxa-desktop.exe"
$dllSrc = Join-Path $targetDir "WebView2Loader.dll"
$dllFallback = Join-Path $PSScriptRoot "src-tauri\WebView2Loader.dll"

if (-not (Test-Path $exeSrc)) {
    throw "Missing $exeSrc - run .\build-gnu.ps1 first."
}
if (-not (Test-Path $dllSrc)) {
    if (-not (Test-Path $dllFallback)) {
        throw "Missing WebView2Loader.dll in target\release and src-tauri."
    }
    $dllSrc = $dllFallback
}

$stage = Join-Path $distDir "jyroxa-portable-$([guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Path $stage -Force | Out-Null
try {
    Copy-Item $exeSrc (Join-Path $stage "Jyroxa.exe") -Force
    Copy-Item $dllSrc (Join-Path $stage "WebView2Loader.dll") -Force

    New-Item -ItemType Directory -Path $distDir -Force | Out-Null
    $zipPath = Join-Path $distDir "Jyroxa-$version-windows-x64-portable.zip"
    if (Test-Path -LiteralPath $zipPath) { Remove-Item -LiteralPath $zipPath -Force }
    Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zipPath -CompressionLevel Optimal
    Write-Host "Wrote $zipPath"
} finally {
    if ([IO.Path]::GetDirectoryName($stage) -ne $distDir.TrimEnd('\')) { throw 'Unexpected portable staging directory' }
    Remove-Item -LiteralPath $stage -Recurse -Force -ErrorAction SilentlyContinue
}
