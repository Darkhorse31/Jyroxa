# Package a portable Windows x64 zip: Jyroxa.exe + WebView2Loader.dll (+ native host).
$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$tauriConf = Join-Path $PSScriptRoot "src-tauri\tauri.conf.json"
$conf = Get-Content $tauriConf -Raw | ConvertFrom-Json
$version = $conf.version
$targetRoot = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $repoRoot "target" }
if (-not [System.IO.Path]::IsPathRooted($targetRoot)) { $targetRoot = Join-Path $repoRoot $targetRoot }
$targetDir = Join-Path $targetRoot "release"
$resourceDir = if ($env:JYROXA_VERIFIED_SIDECARS_DIR) { $env:JYROXA_VERIFIED_SIDECARS_DIR } else { Join-Path $PSScriptRoot "src-tauri" }
$distDir = Join-Path $repoRoot "dist"
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

$stage = Join-Path $env:TEMP "jyroxa-portable-$([guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Path $stage -Force | Out-Null
try {
    Copy-Item $exeSrc (Join-Path $stage "Jyroxa.exe") -Force
    Copy-Item $dllSrc (Join-Path $stage "WebView2Loader.dll") -Force
    $hostSrc = Join-Path $resourceDir "jyroxa-native-host.exe"
    if (Test-Path $hostSrc) {
        Copy-Item $hostSrc (Join-Path $stage "jyroxa-native-host.exe") -Force
    } else {
        throw "Missing required portable resource $hostSrc"
    }
    $sidecarStage = Join-Path $stage "sidecars"
    New-Item -ItemType Directory -Path $sidecarStage -Force | Out-Null
    $cliSrc = Join-Path $resourceDir "jyroxa.exe"
    if (Test-Path $cliSrc) {
        Copy-Item $cliSrc (Join-Path $sidecarStage "jyroxa.exe") -Force
    } else {
        throw "Missing required portable resource $cliSrc"
    }
    $sidecarDir = $resourceDir
    foreach ($name in @("yt-dlp.exe", "ffmpeg.exe")) {
        $p = Join-Path $sidecarDir $name
        if (-not (Test-Path $p)) { throw "Missing required portable resource $p" }
        Copy-Item $p (Join-Path $sidecarStage $name) -Force
    }

    New-Item -ItemType Directory -Path $distDir -Force | Out-Null
    $zipPath = Join-Path $distDir "Jyroxa-$version-windows-x64-portable.zip"
    if (Test-Path $zipPath) { Remove-Item $zipPath -Force }
    Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zipPath -CompressionLevel Optimal
    Write-Host "Wrote $zipPath"
} finally {
    $resolvedStage = [System.IO.Path]::GetFullPath($stage)
    $resolvedTemp = [System.IO.Path]::GetFullPath($env:TEMP).TrimEnd('\') + '\'
    if (-not $resolvedStage.StartsWith($resolvedTemp, [System.StringComparison]::OrdinalIgnoreCase) -or
        -not ([System.IO.Path]::GetFileName($resolvedStage)).StartsWith('jyroxa-portable-')) {
        throw "Refusing to remove an unexpected portable staging directory: $resolvedStage"
    }
    Remove-Item -LiteralPath $resolvedStage -Recurse -Force -ErrorAction SilentlyContinue
}
