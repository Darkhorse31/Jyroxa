# Rebuild release helpers before Tauri copies resources into the installer.
# This prevents an old, gitignored registrar from being shipped with new sources.
$ErrorActionPreference = 'Stop'
$Workspace = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $Workspace
try {
    # Windows PowerShell turns redirected native stderr (including successful
    # Cargo progress) into error records. Use the exit code to decide success.
    $ErrorActionPreference = 'Continue'
    cargo build --release -p jyroxa-native-host -p jyroxa-cli --bins -j 3
    $BuildExit = $LASTEXITCODE
    $ErrorActionPreference = 'Stop'
    if ($BuildExit -ne 0) { throw "Sidecar build failed: $BuildExit" }
    $Target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $Workspace 'target' }
    if ($env:CARGO_BUILD_TARGET) { $Target = Join-Path $Target $env:CARGO_BUILD_TARGET }
    $Release = Join-Path $Target 'release'
    $Destination = Join-Path $Workspace 'apps/desktop/src-tauri'
    foreach ($Name in @('jyroxa-native-host.exe', 'jyroxa-browser-register.exe', 'jyroxa.exe')) {
        Copy-Item -LiteralPath (Join-Path $Release $Name) -Destination (Join-Path $Destination $Name) -Force
    }
    & (Join-Path $PSScriptRoot 'fetch-video-sidecars.ps1')
} finally {
    Pop-Location
}
