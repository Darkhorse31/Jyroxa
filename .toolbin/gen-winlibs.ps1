# Generates correct MinGW-style import archives for the DLLs that the
# raw-dylib-free build needs (see .cargo/config.toml). For each requested DLL:
#   1. read its real export table from C:\Windows\System32 with llvm-objdump,
#   2. write a .def listing those exports,
#   3. run llvm-dlltool in plain archive mode (-d/-D/-l only) to emit an import
#      archive named after the verbatim DLL into .toolbin/winlibs.
#
# System DLLs are read-only and never modified; only files under
# .toolbin/winlibs are written. Usage (from repo root):
#   powershell -ExecutionPolicy Bypass -File .toolbin\gen-winlibs.ps1 dll1 dll2 ...
param([Parameter(ValueFromRemainingArguments = $true)][string[]]$Dlls)

$ErrorActionPreference = "Stop"
if (-not $env:RUSTUP_HOME) {
    $env:RUSTUP_HOME = Join-Path $env:USERPROFILE ".rustup"
}
$toolchainBin = Join-Path $env:RUSTUP_HOME "toolchains\stable-x86_64-pc-windows-gnu\lib\rustlib\x86_64-pc-windows-gnu\bin"
$objdump = Join-Path $toolchainBin "llvm-objdump.exe"
$dlltool = Join-Path $toolchainBin "llvm-dlltool.exe"
$toolBin = $PSScriptRoot
$winlibs = Join-Path $toolBin "winlibs"
$defdir  = Join-Path $toolBin "defs"
New-Item -ItemType Directory -Force $winlibs | Out-Null
New-Item -ItemType Directory -Force $defdir  | Out-Null

function Find-SystemDll([string]$name) {
    foreach ($dir in @("$env:WINDIR\System32", "$env:WINDIR\SysWOW64")) {
        $p = Join-Path $dir $name
        if (Test-Path $p) { return $p }
    }
    return $null
}

foreach ($dll in $Dlls) {
    $sys = Find-SystemDll $dll
    if (-not $sys) {
        Write-Warning "no system copy of $dll found; writing empty import stub"
        $exports = @()
    } else {
        $raw = & $objdump --private-headers $sys 2>$null
        # llvm-objdump prints export entries as: "<ordinal> <rva> <name>".
        $exports = @()
        $inExport = $false
        foreach ($line in $raw) {
            if ($line -match "Export Table:") { $inExport = $true; continue }
            if ($inExport) {
                if ($line -match "Table:" -and $line -notmatch "Export Table:") { break }
                if ($line -match '^\s*\d+\s+0x[0-9a-fA-F]+\s+(\S+)\s*$') { $exports += $Matches[1] }
            }
        }
    }

    $def = Join-Path $defdir ($dll + ".def")
    $sb = New-Object System.Text.StringBuilder
    [void]$sb.AppendLine("LIBRARY $dll")
    [void]$sb.AppendLine("EXPORTS")
    foreach ($e in $exports) { [void]$sb.AppendLine($e) }
    Set-Content -Path $def -Value $sb.ToString() -Encoding ASCII

    $out = Join-Path $winlibs $dll
    & $dlltool -m i386:x86-64 -d $def -D $dll -l $out
    if ($LASTEXITCODE -ne 0) { throw "dlltool failed for $dll" }
    Write-Host ("generated {0} ({1} exports)" -f $dll, $exports.Count)
}
