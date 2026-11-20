[CmdletBinding()]
param([string]$OutputDir = 'C:\Users\lenovo\Jyroxa-dist')
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$output = [IO.Path]::GetFullPath($OutputDir)
$oldJobs = $env:CARGO_BUILD_JOBS
$oldWrapper = $env:RUSTC_WRAPPER
$oldTarget = $env:CARGO_TARGET_DIR
$artifacts = New-Object 'System.Collections.Generic.List[string]'
function Copy-Artifact([string]$Source, [string]$Name) {
    if (-not (Test-Path -LiteralPath $Source -PathType Leaf)) { throw "Missing build artifact: $Source" }
    $destination = Join-Path $output $Name
    Copy-Item -LiteralPath $Source -Destination $destination -Force
    $artifacts.Add($destination)
}
Push-Location $repo
try {
    $env:CARGO_BUILD_JOBS = '3'
    $target = if ($env:CARGO_TARGET_DIR) { [IO.Path]::GetFullPath($env:CARGO_TARGET_DIR) } else { Join-Path $repo 'target' }
    $env:CARGO_TARGET_DIR = $target
    New-Item -ItemType Directory -Path $output -Force | Out-Null
    $manifest = Join-Path $output 'SHA256SUMS.txt'
    if (Test-Path -LiteralPath $manifest) { Remove-Item -LiteralPath $manifest -Force }
    $version = (Get-Content apps/desktop/src-tauri/tauri.conf.json -Raw | ConvertFrom-Json).version
    if ($version -ne '0.1.1') { throw "Expected version 0.1.1, found $version" }
    cargo build --release -p jyroxa-cli -j 3
    if ($LASTEXITCODE -ne 0) { throw 'CLI release build failed' }
    $stage = Join-Path $output ('cli-stage-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $stage | Out-Null
    try {
        Copy-Item -LiteralPath (Join-Path $target 'release\jyroxa.exe') -Destination $stage
        Copy-Item -LiteralPath (Join-Path $repo 'README.md'), (Join-Path $repo 'LICENSE') -Destination $stage
        $zip = Join-Path $output "Jyroxa-$version-windows-x64-cli.zip"
        Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $zip -Force
        $artifacts.Add($zip)
    } finally {
        if ([IO.Path]::GetDirectoryName($stage) -ne $output.TrimEnd('\')) { throw 'Unexpected staging directory' }
        Remove-Item -LiteralPath $stage -Recurse -Force
    }
    Push-Location (Join-Path $repo 'apps\desktop')
    try {
        npm ci
        if ($LASTEXITCODE -ne 0) { throw 'Desktop dependency installation failed' }
        & .\build-gnu.ps1
        & .\package-portable.ps1 -OutputDir $output
        $artifacts.Add((Join-Path $output "Jyroxa-$version-windows-x64-portable.zip"))
    } finally { Pop-Location }
    $installers = @(Get-ChildItem -LiteralPath (Join-Path $target 'release\bundle\nsis') -Filter "*${version}*.exe" -File)
    if ($installers.Count -ne 1) { throw 'Expected exactly one NSIS installer for this version' }
    Copy-Artifact $installers[0].FullName "Jyroxa-$version-windows-x64-setup.exe"
    Push-Location (Join-Path $repo 'android')
    try {
        $env:RUSTC_WRAPPER = ''
        $variant = if (Test-Path -LiteralPath 'keystore.properties') { 'Release' } else { 'Debug' }
        & .\gradlew.bat buildCargoNdk "assemble$variant" --max-workers=3 --console=plain
        if ($LASTEXITCODE -ne 0) { throw "Android $variant build failed" }
        $lower = $variant.ToLowerInvariant()
        Copy-Artifact (Join-Path $repo "android\app\build\outputs\apk\$lower\app-$lower.apk") "Jyroxa-$version-android-$lower.apk"
    } finally { Pop-Location }
    $artifacts | Sort-Object | ForEach-Object {
        $hash = (Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLowerInvariant()
        "$hash  $([IO.Path]::GetFileName($_))"
    } | Set-Content -LiteralPath $manifest -Encoding ASCII
    Write-Host "Distribution written to $output"
} finally {
    $env:CARGO_BUILD_JOBS = $oldJobs
    $env:RUSTC_WRAPPER = $oldWrapper
    $env:CARGO_TARGET_DIR = $oldTarget
    Pop-Location
}
