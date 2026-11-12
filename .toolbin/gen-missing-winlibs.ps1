# Generate import archives for every `cannot find -l:<dll>` reported in a build log.
# API-set names (api-ms-win-*) have no System32 file; their exports are taken from the
# implementing DLL (KernelBase by default, combase for WinRT, gdi32 for d3dkmt).
# Usage: powershell -ExecutionPolicy Bypass -File .toolbin\gen-missing-winlibs.ps1 <build.log>
param([Parameter(Mandatory)][string]$LogPath)
$ErrorActionPreference = "Stop"
$winlibs = Join-Path $PSScriptRoot "winlibs"
$missing = Select-String -Path $LogPath -Pattern 'cannot find -l:(\S+?):' -AllMatches |
    ForEach-Object { $_.Matches } | ForEach-Object { $_.Groups[1].Value } | Sort-Object -Unique |
    Where-Object { -not (Test-Path (Join-Path $winlibs $_)) }
foreach ($dll in $missing) {
    if ($dll -like 'api-ms-win-*' -or $dll -like 'ext-ms-win-*') {
        $src = "$env:WINDIR\System32\KernelBase.dll"
        if ($dll -like '*winrt*' -or $dll -like '*-com-*') { $src = "$env:WINDIR\System32\combase.dll" }
        elseif ($dll -like '*d3dkmt*' -or $dll -like '*gdi*') { $src = "$env:WINDIR\System32\gdi32.dll" }
        elseif ($dll -like '*shcore*') { $src = "$env:WINDIR\System32\shcore.dll" }
        & (Join-Path $PSScriptRoot "gen-from-path.ps1") $src $dll
    } else {
        & (Join-Path $PSScriptRoot "gen-winlibs.ps1") $dll
    }
}
Write-Host ("{0} import archive(s) generated" -f @($missing).Count)