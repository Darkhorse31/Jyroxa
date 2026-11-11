param(
    [Parameter(Mandatory)][string]$SysPath,
    [Parameter(Mandatory)][string]$DllName
)
$ErrorActionPreference = "Stop"
if (-not $env:RUSTUP_HOME) { $env:RUSTUP_HOME = Join-Path $env:USERPROFILE ".rustup" }
$toolchainBin = Join-Path $env:RUSTUP_HOME "toolchains\stable-x86_64-pc-windows-gnu\lib\rustlib\x86_64-pc-windows-gnu\bin"
$objdump = Join-Path $toolchainBin "llvm-objdump.exe"
$dlltool = Join-Path $toolchainBin "llvm-dlltool.exe"
$def = Join-Path (Join-Path $PSScriptRoot "defs") ($DllName + ".def")
$out = Join-Path (Join-Path $PSScriptRoot "winlibs") $DllName
$raw = & $objdump -p $SysPath 2>$null
$exports = @()
$inExport = $false
foreach ($line in $raw) {
    if ($line -match "Export Table:") { $inExport = $true; continue }
    if ($inExport) {
        if ($line -match "Table:" -and $line -notmatch "Export Table:") { break }
        if ($line -match '^\s*\d+\s+0x[0-9a-fA-F]+\s+(\S+)\s*$') { $exports += $Matches[1] }
        elseif ($line -match '^\s*\d+\s+([A-Za-z_]\S*)\s*$') { $exports += $Matches[1] }
    }
}
$sb = New-Object System.Text.StringBuilder
[void]$sb.AppendLine("LIBRARY $DllName")
[void]$sb.AppendLine("EXPORTS")
foreach ($e in $exports) { [void]$sb.AppendLine($e) }
Set-Content -Path $def -Value $sb.ToString() -Encoding ASCII
& $dlltool -m i386:x86-64 -d $def -D $DllName -l $out
if ($LASTEXITCODE -ne 0) { throw "dlltool failed" }
Write-Host "generated $DllName ($($exports.Count) exports)"
