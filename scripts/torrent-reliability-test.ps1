$ErrorActionPreference = "Stop"
. C:\Users\lenovo\.jyroxa-tools\env.ps1
$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$jyroxa = Join-Path $root "target\release\jyroxa.exe"
if (-not (Test-Path $jyroxa)) { throw "build jyroxa.exe first" }

$benchRoot = Join-Path $env:TEMP "jyroxa-torrent-reliability"
Remove-Item -Recurse -Force $benchRoot -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Path $benchRoot | Out-Null

$debianTorrent = "https://cdimage.debian.org/debian-cd/current/amd64/bt-cd/debian-13.7.0-amd64-netinst.iso.torrent"
$ubuntuTorrent = "https://releases.ubuntu.com/24.04.3/ubuntu-24.04.3-desktop-amd64.iso.torrent"
$blenderTorrent = "https://download.blender.org/demo/movies/BBB/bbb_sunflower_1080p_30fps_stereo_abl16.mp4.torrent"
# DHT-only Debian netinst (no tr=)
$debianDhtMagnet = "magnet:?xt=urn:btih:8cbf5c46714d0c5fd7df22c87a5cfab3d540c74d"
$bigBuckMagnet = "magnet:?xt=urn:btih:dd8255dc81c8117f5eecb9d6bfdb10faea1d4a1e&dn=Big+Buck+Bunny&tr=udp%3A%2F%2Fexplodie.org%3A6969&tr=udp%3A%2F%2Ftracker.coppersurfer.tk%3A6969&tr=udp%3A%2F%2Ftracker.empire-js.us%3A1337&tr=udp%3A%2F%2Ftracker.opentrackr.org%3A1337&tr=udp%3A%2F%2Ftracker.leechers-paradise.org%3A6969&tr=udp%3A%2F%2Ftracker.pirateparty.gr%3A6969&tr=udp%3A%2F%2Ftracker.cyberia.is%3A6969&tr=udp%3A%2F%2Fopen.demonii.si%3A1337"

$cases = @(
    @{ Name = "debian-torrent"; Source = $debianTorrent; CapSec = 90 }
    @{ Name = "ubuntu-torrent"; Source = $ubuntuTorrent; CapSec = 120 }
    @{ Name = "blender-torrent"; Source = $blenderTorrent; CapSec = 120 }
    @{ Name = "debian-dht-magnet"; Source = $debianDhtMagnet; CapSec = 180 }
    @{ Name = "big-buck-bunny"; Source = $bigBuckMagnet; CapSec = 120 }
)

$rows = @()
foreach ($c in $cases) {
    $dir = Join-Path $benchRoot $c.Name
    New-Item -ItemType Directory -Path $dir -Force | Out-Null
    Write-Host "=== meta: $($c.Name) ==="
    $prevEa = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    $metaOut = & $jyroxa torrent meta $c.Source 2>&1 | Out-String
    $metaExit = $LASTEXITCODE
    $ErrorActionPreference = $prevEa
    $metaOk = $metaExit -eq 0
    if (-not $metaOk) { Write-Warning "meta failed: $metaOut" }
    $metaName = ""
    if ($metaOk -and ($metaOut -match '"name"\s*:\s*"([^"]+)"')) { $metaName = $Matches[1] }

    Write-Host "=== get (cap $($c.CapSec)s): $($c.Name) ==="
    $logFile = Join-Path $dir "stderr.log"
    $job = Start-Job -ScriptBlock {
        param($Exe, $Src, $OutDir, $Log)
        & $Exe torrent get $Src -o $OutDir 2>&1 | Tee-Object -FilePath $Log
    } -ArgumentList $jyroxa, $c.Source, $dir, $logFile
    Wait-Job $job -Timeout $c.CapSec | Out-Null
    Stop-Job $job -ErrorAction SilentlyContinue
    Remove-Job $job -Force

    $log = ""
    if (Test-Path $logFile) { $log = Get-Content $logFile -Raw }
    $metaMs = if ($log -match "metadata: (\d+) ms") { [int]$Matches[1] } else { $null }
    $hadMetadataProgress = $log -match "phase=metadata"
    $maxSpeed = 0.0
    foreach ($line in ($log -split "`n")) {
        if ($line -match "speed_bps=(\d+)") {
            $sp = [double]$Matches[1]
            if ($sp -gt $maxSpeed) { $maxSpeed = $sp }
        }
    }
    $maxMiB = [math]::Round($maxSpeed / (1024 * 1024), 2)
    $bytes = (Get-ChildItem $dir -Recurse -File -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -notmatch '\\.jyroxa-torrent-session\\' } |
        Measure-Object -Property Length -Sum).Sum
    if (-not $bytes) { $bytes = 0 }

    $rows += [pscustomobject]@{
        Name = $c.Name
        MetaOk = $metaOk
        MetaName = $metaName
        MetaMs = $metaMs
        MetadataProgress = $hadMetadataProgress
        MaxMiB = $maxMiB
        Bytes = $bytes
    }
}

$rows | Format-Table -AutoSize
$report = Join-Path $benchRoot "results.tsv"
$rows | Export-Csv -NoTypeInformation -Delimiter "`t" $report
Write-Host "Wrote $report"