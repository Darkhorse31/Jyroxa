# Build GNU Windows linker helpers (.toolbin/dlltool-wrap.exe, .toolbin/rustc-wrap.exe).
$ErrorActionPreference = "Stop"
$toolBin = $PSScriptRoot
if (-not $env:RUSTUP_HOME) { $env:RUSTUP_HOME = Join-Path $env:USERPROFILE ".rustup" }
$rustc = Join-Path $env:RUSTUP_HOME "toolchains\stable-x86_64-pc-windows-gnu\bin\rustc.exe"
if (-not (Test-Path $rustc)) {
    throw "rustc for x86_64-pc-windows-gnu not found (rustup target add x86_64-pc-windows-gnu)"
}
foreach ($pair in @(
    @("dlltool_wrap.rs", "dlltool-wrap.exe"),
    @("rustc_wrap.rs", "rustc-wrap.exe")
)) {
    & $rustc (Join-Path $toolBin $pair[0]) -O -o (Join-Path $toolBin $pair[1])
    if ($LASTEXITCODE -ne 0) { throw "rustc failed building $($pair[1])" }
}
$llvm = Join-Path $env:RUSTUP_HOME "toolchains\stable-x86_64-pc-windows-gnu\lib\rustlib\x86_64-pc-windows-gnu\bin\llvm-dlltool.exe"
if (Test-Path $llvm) {
    Copy-Item -Force $llvm (Join-Path $toolBin "llvm-dlltool.exe")
}
Write-Host "built dlltool-wrap.exe and rustc-wrap.exe under $toolBin"
