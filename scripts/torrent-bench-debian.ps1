$ErrorActionPreference = "Stop"
. C:\Users\lenovo\.jyroxa-tools\env.ps1
$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$jyroxa = Join-Path $root "target\release\jyroxa.exe"
$aria2 = "C:\Users\lenovo\.jyroxa-tools\bin\aria2c.exe"
$torrent = "https://cdimage.debian.org/debian-cd/current/amd64/bt-cd/debian-13.7.0-amd64-netinst.iso.torrent"
$benchRoot = Join-Path $env:TEMP "jyroxa-torrent-bench-016"
if (Test-Path $benchRoot) { Remove-Item -Recurse -Force $benchRoot }
New-Item -ItemType Directory -Path $benchRoot | Out-Null

$trackers = @(
    "udp://tracker.opentrackr.org:1337/announce",
    "udp://open.tracker.cl:1337/announce",
    "udp://tracker.torrent.eu.org:451/announce",
    "udp://explodie.org:6969/announce",
    "udp://tracker1.bt.moack.co.kr:80/announce",
    "udp://tracker.theoks.net:6969/announce",
    "udp://opentracker.i2p.rocks:6969/announce",
    "udp://tracker-udp.gbitt.info:80/announce",
    "udp://run.publictracker.xyz:6969/announce",
    "udp://public.tracker.vraphim.com:6969/announce",
    "udp://tracker.tiny-vps.com:6969/announce",
    "udp://open.demonii.com:1337/announce",
    "udp://tracker.openbittorrent.com:6969/announce",
    "udp://exodus.desync.com:6969/announce",
    "udp://open.stealth.si:80/announce",
    "udp://tracker.moeking.me:6969/announce",
    "udp://ipv4.tracker.harry.lu:80/announce",
    "udp://retracker01-msk-virt.corbina.net:80/announce",
    "udp://tracker.denise.hlawrence.com:6969/announce",
    "https://tracker.tamersunion.org:443/announce",
    "http://tracker.openbittorrent.com:80/announce",
    "http://tracker.opentrackr.org:1337/announce",
    "http://open.acgnxtracker.com:80/announce",
    "http://www.peckrocks.com:6969/announce"
) -join ","
$ariaFlags = @(
    "--bt-max-peers=200",
    "--bt-request-peer-speed-limit=50M",
    "--max-overall-download-limit=0",
    "--enable-dht=true",
    "--bt-enable-lpd=true",
    "--enable-peer-exchange=true",
    "--bt-tracker=$trackers",
    "--seed-time=0",
    "--max-connection-per-server=16"
)

$rows = @()
for ($run = 1; $run -le 3; $run++) {
    $dir = Join-Path $benchRoot "jyroxa-$run"
    Remove-Item -Recurse -Force $dir -ErrorAction SilentlyContinue
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $metaMs = $null
    $out = (& $jyroxa torrent get $torrent -o $dir *>&1 | ForEach-Object { "$_" }) -join "`n"
    $sw.Stop()
    if ($out -match "metadata: (\d+) ms") { $metaMs = [int]$Matches[1] }
    $totalSec = $sw.Elapsed.TotalSeconds
    $avg = 0.0
    if ($out -match "done in ([0-9.]+)s avg ([0-9.]+) MiB/s") {
        $avg = [double]$Matches[2]
    }
    $rows += [pscustomobject]@{ Client="Jyroxa"; Run=$run; MetadataMs=$metaMs; TotalSec=[math]::Round($totalSec,1); AvgMiB=$avg }
    Remove-Item -Recurse -Force $dir -ErrorAction SilentlyContinue
}

for ($run = 1; $run -le 3; $run++) {
    $dir = Join-Path $benchRoot "aria2-$run"
    Remove-Item -Recurse -Force $dir -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Path $dir | Out-Null
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    & $aria2 @ariaFlags -d $dir -o "debian.iso" $torrent 2>&1 | Out-Null
    $sw.Stop()
    $totalSec = $sw.Elapsed.TotalSeconds
    $iso = Join-Path $dir "debian.iso"
    $bytes = if (Test-Path $iso) { (Get-Item $iso).Length } else { 0 }
    $avg = if ($totalSec -gt 0) { ($bytes / $totalSec) / (1024*1024) } else { 0 }
    $rows += [pscustomobject]@{ Client="aria2c"; Run=$run; MetadataMs=$null; TotalSec=[math]::Round($totalSec,1); AvgMiB=[math]::Round($avg,2) }
    Remove-Item -Recurse -Force $dir -ErrorAction SilentlyContinue
}

Remove-Item -Recurse -Force $benchRoot -ErrorAction SilentlyContinue
$log = Join-Path $root "reports\torrent-bench-0.1.8.tsv"
$rows | Format-Table -AutoSize | Out-String | Write-Host
$rows | Export-Csv -NoTypeInformation -Delimiter "`t" $log
Write-Host "Wrote $log"