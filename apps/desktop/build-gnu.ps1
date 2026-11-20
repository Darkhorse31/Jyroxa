# Build Jyroxa desktop on x86_64-pc-windows-gnu (no MSVC).
$ErrorActionPreference = 'Stop'
if (-not $env:ANDROID_NDK_HOME) { throw 'Set ANDROID_NDK_HOME to the Android NDK directory.' }
$ndkBin = Join-Path $env:ANDROID_NDK_HOME 'toolchains\llvm\prebuilt\windows-x86_64\bin'
$toolBin = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..\.toolbin'))
if (-not (Test-Path -LiteralPath $toolBin -PathType Container)) { throw "Missing repository tool directory: $toolBin" }
foreach ($tool in @(@('llvm-windres.exe', 'windres.exe'), @('llvm-ar.exe', 'ar.exe'))) {
    $source = Join-Path $ndkBin $tool[0]
    if (-not (Test-Path -LiteralPath $source -PathType Leaf)) { throw "Missing NDK tool: $source. Check ANDROID_NDK_HOME." }
    $destination = Join-Path $toolBin $tool[1]
    if (-not (Test-Path -LiteralPath $destination)) { Copy-Item -LiteralPath $source -Destination $destination }
}
$oldPath = $env:PATH
$oldJobs = $env:CARGO_BUILD_JOBS
try {
    $env:PATH = "$toolBin;$ndkBin;$oldPath"
    $env:CARGO_BUILD_JOBS = '3'
    Push-Location $PSScriptRoot
    try {
        if ($args -contains '--no-bundle') { npm run tauri build -- --no-bundle }
        else { npm run tauri build -- --bundles nsis }
        if ($LASTEXITCODE -ne 0) { throw "Desktop build failed with exit code $LASTEXITCODE" }
    } finally { Pop-Location }
} finally {
    $env:PATH = $oldPath
    $env:CARGO_BUILD_JOBS = $oldJobs
}
