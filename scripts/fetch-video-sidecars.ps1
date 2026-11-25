# Download yt-dlp and ffmpeg essentials for Windows x64 bundling.
$ErrorActionPreference = "Stop"
$Dest = Join-Path $PSScriptRoot "..\apps\desktop\src-tauri"
New-Item -ItemType Directory -Force -Path $Dest | Out-Null

# Pin the Windows sidecar that fixes YouTube's player challenge regression.
# Never preserve a stale yt-dlp.exe merely because a previous build left it here.
$SidecarLock = Get-Content (Join-Path $PSScriptRoot 'video-sidecars.lock.json') -Raw | ConvertFrom-Json
$YtVersion = [string]$SidecarLock.ytDlp.version
$YtSha256 = ([string]$SidecarLock.ytDlp.windowsX64Sha256).ToLowerInvariant()
$YtUrl = "https://github.com/yt-dlp/yt-dlp/releases/download/$YtVersion/yt-dlp.exe"

# Ship the existing build runtime so YouTube works on PCs without Node on PATH.
# The official yt-dlp.exe already contains the EJS challenge scripts.
$NodeCommand = Get-Command node.exe -ErrorAction Stop
$NodeVersion = (& $NodeCommand.Source --version).Trim().TrimStart('v')
if ([version]$NodeVersion -lt [version]'22.0.0') { throw 'YouTube sidecars require Node 22 or newer in the build environment.' }
Copy-Item -LiteralPath $NodeCommand.Source -Destination (Join-Path $Dest 'node.exe') -Force
Invoke-WebRequest -Uri "https://raw.githubusercontent.com/nodejs/node/v$NodeVersion/LICENSE" -OutFile (Join-Path $Dest 'node-LICENSE.txt')

$Yt = Join-Path $Dest "yt-dlp.exe"
$YtStage = Join-Path $Dest ".yt-dlp-$YtVersion-$PID.exe"
$CurrentYtVersion = $null
$CurrentYtSha256 = $null
if (Test-Path -LiteralPath $Yt -PathType Leaf) {
    $CurrentYtVersion = (& $Yt --version 2>$null | Select-Object -First 1).Trim()
    $CurrentYtSha256 = (Get-FileHash -LiteralPath $Yt -Algorithm SHA256).Hash.ToLowerInvariant()
}
if ($CurrentYtVersion -ne $YtVersion -or $CurrentYtSha256 -ne $YtSha256) {
    try {
        Invoke-WebRequest -Uri $YtUrl -OutFile $YtStage
        $DownloadedYtSha256 = (Get-FileHash -LiteralPath $YtStage -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($DownloadedYtSha256 -ne $YtSha256) {
            throw "yt-dlp $YtVersion SHA-256 mismatch: $DownloadedYtSha256"
        }
        $DownloadedYtVersion = (& $YtStage --version 2>$null | Select-Object -First 1).Trim()
        if ($DownloadedYtVersion -ne $YtVersion) {
            throw "Expected yt-dlp $YtVersion, received $DownloadedYtVersion"
        }
        Move-Item -LiteralPath $YtStage -Destination $Yt -Force
        Write-Host "Installed verified yt-dlp $YtVersion ($YtSha256)"
    } finally {
        if (Test-Path -LiteralPath $YtStage -PathType Leaf) {
            Remove-Item -LiteralPath $YtStage -Force
        }
    }
} else {
    Write-Host "Verified existing yt-dlp $YtVersion ($YtSha256)"
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
