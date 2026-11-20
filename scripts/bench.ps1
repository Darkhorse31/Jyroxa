# Measure end-to-end download throughput in decimal MB/s, including startup and probing.
[CmdletBinding()]
param(
    [string]$CliPath = 'target\release\jyroxa.exe',
    [string[]]$Urls = @(
        'https://sin-speed.hetzner.com/100MB.bin',
        'https://fsn1-speed.hetzner.com/100MB.bin',
        'https://hel1-speed.hetzner.com/100MB.bin',
        'https://ash-speed.hetzner.com/100MB.bin',
        'https://proof.ovh.net/files/100Mb.dat',
        'http://speedtest.tele2.net/100MB.zip',
        'https://speed.cloudflare.com/__down?bytes=104857600'
    ),
    [ValidateRange(1, 1000)][int]$Repeats = 3,
    [ValidateRange(1, 86400)][int]$TimeoutSeconds = 180,
    [switch]$IncludeFixed16,
    [string]$CsvPath = 'bench.csv'
)
$ErrorActionPreference = 'Stop'
$cli = (Resolve-Path -LiteralPath $CliPath).Path
$csv = [IO.Path]::GetFullPath($CsvPath)
[IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($csv)) | Out-Null
foreach ($url in $Urls) {
    $uri = [uri]$url
    if (-not $uri.IsAbsoluteUri -or $uri.Scheme -notin @('http', 'https')) { throw "Invalid HTTP URL: $url" }
}
if ($Urls.Count -eq 0) { throw 'At least one URL is required.' }
$modes = @('1', 'auto')
if ($IncludeFixed16) { $modes += '16' }
$rows = New-Object 'System.Collections.Generic.List[object]'
$scratchRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\.jyroxa-bench'))
$work = Join-Path $scratchRoot ([guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($work) | Out-Null
try {
    foreach ($url in $Urls) {
        for ($run = 1; $run -le $Repeats; $run++) {
            for ($index = 0; $index -lt $modes.Count; $index++) {
                # Rotate the first mode on successive repeats to balance ordering bias.
                $mode = $modes[($index + $run - 1) % $modes.Count]
                $out = Join-Path $work 'download.bin'
                $stdout = Join-Path $work 'stdout.log'
                $stderr = Join-Path $work 'stderr.log'
                $arguments = @('get', ('"' + $url + '"'), '-o', ('"' + $out + '"'), '-c')
                if ($mode -eq 'auto') { $arguments += '16' } else { $arguments += @($mode, '--no-autotune') }
                $timestamp = [DateTimeOffset]::UtcNow.ToOffset([TimeSpan]::FromMinutes(330)).ToString('yyyy-MM-ddTHH:mm:sszzz')
                $watch = [Diagnostics.Stopwatch]::StartNew()
                $process = $null
                $code = -1
                $notes = ''
                $bytes = $null
                try {
                    $process = Start-Process -FilePath $cli -ArgumentList $arguments -WorkingDirectory $work -PassThru -WindowStyle Hidden -RedirectStandardOutput $stdout -RedirectStandardError $stderr
                    # Retain the native handle so Windows PowerShell can read the exit code.
                    $null = $process.Handle
                    $waitMs = [int][math]::Max(0, [math]::Ceiling(($TimeoutSeconds - $watch.Elapsed.TotalSeconds) * 1000))
                    if (-not $process.WaitForExit($waitMs)) {
                        $process.Kill()
                        $process.WaitForExit()
                        $code = 124
                        $notes = 'timeout; partial file excluded'
                    } else {
                        $process.WaitForExit()
                        $code = $process.ExitCode
                    }
                    $watch.Stop()
                    if ($code -eq 0 -and (Test-Path -LiteralPath $out)) {
                        $bytes = (Get-Item -LiteralPath $out).Length
                    } elseif ($code -ne 124) {
                        $notes = ''
                        if (Test-Path -LiteralPath $stderr) {
                            $diagnostics = Get-Content -LiteralPath $stderr -Raw -Encoding UTF8
                            if ($diagnostics) { $notes = $diagnostics.Trim() }
                        }
                        if (-not $notes) { $notes = 'Download failed or output file missing' }
                        if ($code -eq 0) { $code = -1 }
                    }
                } catch {
                    $code = -1
                    $bytes = $null
                    $notes = $_.Exception.Message
                } finally {
                    $watch.Stop()
                    if ($null -ne $process) {
                        if (-not $process.HasExited) { $process.Kill(); $process.WaitForExit() }
                        $process.Dispose()
                    }
                }
                $seconds = $watch.Elapsed.TotalSeconds
                $speed = $null
                if ($null -ne $bytes) { $speed = [math]::Round($bytes / $seconds / 1000000, 6) }
                $row = [pscustomobject][ordered]@{
                    timestamp_ist = $timestamp; server = ([uri]$url).Host; url = $url
                    mode = $mode; run = $run; bytes = $bytes
                    seconds = $seconds.ToString('F6', [Globalization.CultureInfo]::InvariantCulture)
                    mb_per_s = if ($null -ne $speed) { $speed.ToString('F6', [Globalization.CultureInfo]::InvariantCulture) } else { '' }
                    exit_code = $code; notes = $notes
                }
                $rows.Add($row)
                if ($rows.Count -eq 1) { $row | Export-Csv -LiteralPath $csv -NoTypeInformation -Encoding UTF8 }
                else { $row | Export-Csv -LiteralPath $csv -NoTypeInformation -Encoding UTF8 -Append }
                # Clear per-host tuning history too, giving every measurement a fresh start.
                Get-ChildItem -LiteralPath $work -Force | ForEach-Object {
                    if ([IO.Path]::GetDirectoryName($_.FullName) -ne $work) { throw 'Unexpected run artifact path' }
                    Remove-Item -LiteralPath $_.FullName -Recurse -Force
                }
                Write-Host "$($row.server) mode=$mode run=$run exit=$code MB/s=$($row.mb_per_s)"
            }
        }
    }
} finally {
    # Only the unique directory created by this invocation is removed.
    if ([IO.Path]::GetDirectoryName($work) -ne $scratchRoot) { throw 'Unexpected temporary directory' }
    Remove-Item -LiteralPath $work -Recurse -Force
}
$rows | Group-Object server, mode | ForEach-Object {
    $samples = @($_.Group | Where-Object { $_.exit_code -eq 0 } | ForEach-Object { [double]::Parse($_.mb_per_s, [Globalization.CultureInfo]::InvariantCulture) } | Sort-Object)
    $median = $null
    if ($samples.Count -gt 0) {
        $middle = [int][math]::Floor($samples.Count / 2)
        if ($samples.Count % 2) { $median = $samples[$middle] }
        else { $median = ($samples[$middle - 1] + $samples[$middle]) / 2 }
    }
    [pscustomobject]@{ server = $_.Group[0].server; mode = $_.Group[0].mode; successful = $samples.Count; failed = $_.Count - $samples.Count; median_mb_per_s = $median }
} | Format-Table -AutoSize | Out-Host
Write-Host "CSV: $csv"
