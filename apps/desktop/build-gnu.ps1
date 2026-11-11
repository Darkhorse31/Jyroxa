# Build SwiftGet desktop on x86_64-pc-windows-gnu (no MSVC).
# Requires .toolbin/winlibs (see docs/BUILD_WINDOWS_GNU.md) and llvm windres on PATH.
$ErrorActionPreference = "Stop"
$ndkBin = "E:\.swiftget-tools\android-sdk\ndk\27.2.12479018\toolchains\llvm\prebuilt\windows-x86_64\bin"
$toolBin = "E:\SwiftGet\.toolbin"
if (-not (Test-Path (Join-Path $toolBin "windres.exe"))) {
    Copy-Item (Join-Path $ndkBin "llvm-windres.exe") (Join-Path $toolBin "windres.exe") -Force
}
if (-not (Test-Path (Join-Path $toolBin "ar.exe"))) {
    Copy-Item (Join-Path $ndkBin "llvm-ar.exe") (Join-Path $toolBin "ar.exe") -Force
}
$env:PATH = "$toolBin;$ndkBin;" + $env:PATH
Set-Location $PSScriptRoot
$bundle = $true
if ($args -contains "--no-bundle") { $bundle = $false }
if ($bundle) {
    npm run tauri build
} else {
    npm run tauri build -- --no-bundle
}
