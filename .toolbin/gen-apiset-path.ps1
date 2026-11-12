# Import archive for api-ms-win-core-path-l1-1-0.dll (API set; symbols live in KernelBase).
$ErrorActionPreference = "Stop"
if (-not $env:RUSTUP_HOME) { $env:RUSTUP_HOME = Join-Path $env:USERPROFILE ".rustup" }
$toolchainBin = Join-Path $env:RUSTUP_HOME "toolchains\stable-x86_64-pc-windows-gnu\lib\rustlib\x86_64-pc-windows-gnu\bin"
$objdump = Join-Path $toolchainBin "llvm-objdump.exe"
$dlltool = Join-Path $toolchainBin "llvm-dlltool.exe"
$dllName = "api-ms-win-core-path-l1-1-0.dll"
$def = Join-Path (Join-Path $PSScriptRoot "defs") ($dllName + ".def")
$out = Join-Path (Join-Path $PSScriptRoot "winlibs") $dllName
$raw = & $objdump -p "C:\Windows\System32\KernelBase.dll" 2>$null
$exports = @()
$inExport = $false
foreach ($line in $raw) {
    if ($line -match "Export Table:") { $inExport = $true; continue }
    if ($inExport) {
        if ($line -match "Table:" -and $line -notmatch "Export Table:") { break }
        if ($line -match '^\s*\d+\s+(?:0x[0-9a-fA-F]+\s+)?(\S+)(?:\s+\(forwarded to [^)]*\))?\s*$') { $name = $Matches[1]; if ($name -like "PathCch*") { $exports += $name } }
    }
}
$sb = New-Object System.Text.StringBuilder
[void]$sb.AppendLine("LIBRARY $dllName")
[void]$sb.AppendLine("EXPORTS")
foreach ($e in $exports) { [void]$sb.AppendLine($e) }
Set-Content -Path $def -Value $sb.ToString() -Encoding ASCII
& $dlltool -m i386:x86-64 -d $def -D $dllName -l $out
Write-Host "generated $dllName ($($exports.Count) PathCch exports)"
