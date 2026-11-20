# Offline regression checks for ordering, quoting, failures, timeouts and cleanup.
$ErrorActionPreference = 'Stop'
$scratchRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\.jyroxa-bench'))
$root = Join-Path $scratchRoot ('test-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $root | Out-Null
$oldPaths = $env:JYROXA_TEST_PATHS
try {
    $exe = Join-Path $root 'test downloader.exe'
    Add-Type -TypeDefinition @'
using System;
using System.IO;
using System.Threading;
public class TestDownloader {
    public static int Main(string[] args) {
        if (args[0] != "get" || args[2] != "-o" || args[4] != "-c") return 9;
        if (Directory.Exists(".jyroxa")) return 10;
        bool fixedMode = args.Length == 7 && args[6] == "--no-autotune";
        if ((args[5] == "1" || args[5] == "16") && (fixedMode || args.Length == 6)) {
            File.AppendAllText(Environment.GetEnvironmentVariable("JYROXA_TEST_PATHS"), args[3] + "\n");
            File.WriteAllBytes(args[3], new byte[1024]);
            File.WriteAllText(args[3] + ".jxstate", "temporary");
            Directory.CreateDirectory(".jyroxa");
            File.WriteAllText(Path.Combine(".jyroxa", "hoststats.json"), "{}");
            if (args[1].Contains("timeout")) Thread.Sleep(10000);
            if (args[1].Contains("silent")) return 8;
            if (args[1].Contains("failure")) { Console.Error.WriteLine("test failure, quoted \"message\""); return 7; }
            return 0;
        }
        return 8;
    }
}
'@ -OutputAssembly $exe -OutputType ConsoleApplication
    $env:JYROXA_TEST_PATHS = Join-Path $root 'paths.txt'
    $csv = Join-Path $root 'results.csv'
    & (Join-Path $PSScriptRoot 'bench.ps1') -CliPath $exe -Urls @('https://example.test/ok?a=1&b=2') -Repeats 2 -IncludeFixed16 -CsvPath $csv
    $rows = @(Import-Csv -LiteralPath $csv)
    if ($rows.Count -ne 6 -or ($rows.mode -join ',') -ne '1,auto,16,auto,16,1') { throw 'Incorrect interleaving' }
    foreach ($row in $rows) {
        if ($row.exit_code -ne '0' -or $row.bytes -ne '1024' -or [double]$row.mb_per_s -le 0 -or $row.timestamp_ist -notlike '*+05:30') { throw 'Incorrect successful measurement' }
    }
    & (Join-Path $PSScriptRoot 'bench.ps1') -CliPath $exe -Urls @('https://example.test/failure', 'https://example.test/silent') -Repeats 1 -CsvPath $csv
    $rows = @(Import-Csv -LiteralPath $csv)
    if (($rows.exit_code -join ',') -ne '7,7,8,8') { throw "Incorrect failure status: $($rows | ConvertTo-Json -Compress)" }
    foreach ($row in $rows) {
        if ($row.bytes -or $row.mb_per_s -or -not $row.notes) { throw 'Failed download counted as successful' }
    }
    & (Join-Path $PSScriptRoot 'bench.ps1') -CliPath $exe -Urls @('https://example.test/timeout') -Repeats 1 -TimeoutSeconds 1 -CsvPath $csv
    $rows = @(Import-Csv -LiteralPath $csv)
    if (($rows.exit_code -join ',') -ne '124,124') { throw 'Incorrect timeout status' }
    foreach ($row in $rows) {
        if ($row.bytes -or $row.mb_per_s -or -not $row.notes) { throw 'Timed-out download counted as successful' }
    }
    foreach ($path in (Get-Content -LiteralPath $env:JYROXA_TEST_PATHS)) {
        if (Test-Path -LiteralPath ([IO.Path]::GetDirectoryName($path))) { throw 'Temporary download directory leaked' }
    }
    Write-Host 'PASS: ordering, fixed modes, quoted paths/URLs, CSV, failures, timeout and cleanup'
} finally {
    $env:JYROXA_TEST_PATHS = $oldPaths
    if ([IO.Path]::GetDirectoryName($root) -ne $scratchRoot) { throw 'Unexpected test directory' }
    Remove-Item -LiteralPath $root -Recurse -Force
}
