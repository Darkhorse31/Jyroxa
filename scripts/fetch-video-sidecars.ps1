# Download yt-dlp and ffmpeg essentials for Windows x64 bundling.
$ErrorActionPreference = "Stop"
$Dest = Join-Path $PSScriptRoot "..\apps\desktop\src-tauri"
New-Item -ItemType Directory -Force -Path $Dest | Out-Null

# Ship the existing build runtime so YouTube works on PCs without Node on PATH.
# The official yt-dlp.exe already contains the EJS challenge scripts.
$NodeCommand = Get-Command node.exe -ErrorAction Stop
$NodeVersion = (& $NodeCommand.Source --version).Trim().TrimStart('v')
if ([version]$NodeVersion -lt [version]'22.0.0') { throw 'YouTube sidecars require Node 22 or newer in the build environment.' }
Copy-Item -LiteralPath $NodeCommand.Source -Destination (Join-Path $Dest 'node.exe') -Force
Invoke-WebRequest -Uri "https://raw.githubusercontent.com/nodejs/node/v$NodeVersion/LICENSE" -OutFile (Join-Path $Dest 'node-LICENSE.txt')

$Yt = Join-Path $Dest "yt-dlp.exe"
if (-not (Test-Path $Yt)) {
    Invoke-WebRequest -Uri "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe" -OutFile $Yt
    Write-Host "Fetched yt-dlp.exe"
}

$FfZip = Join-Path $env:TEMP "ffmpeg-essentials.zip"
$FfRoot = Join-Path $env:TEMP "ffmpeg-essentials"
if (-not (Test-Path (Join-Path $Dest "ffmpeg.exe"))) {
    Invoke-WebRequest -Uri "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip" -OutFile $FfZip
    if (Test-Path $FfRoot) { Remove-Item $FfRoot -Recurse -Force }
    Expand-Archive -Path $FfZip -DestinationPath $FfRoot -Force
    $FfExe = Get-ChildItem -Path $FfRoot -Filter ffmpeg.exe -Recurse | Select-Object -First 1
    if (-not $FfExe) { throw "ffmpeg.exe not found in archive" }
    Copy-Item $FfExe.FullName (Join-Path $Dest "ffmpeg.exe") -Force
    Write-Host "Fetched ffmpeg.exe"
}
