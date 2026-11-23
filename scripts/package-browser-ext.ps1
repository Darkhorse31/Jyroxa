$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
$Ext = Join-Path $Root "extensions\browser"
$Icons = Join-Path $Ext "icons"
# Brand generation owns these source files. Packaging preserves the exact
# extension assets that were reviewed and tested.
foreach ($Size in @(16, 32, 48, 128)) {
    $Icon = Join-Path $Icons "icon-$Size.png"
    if (-not (Test-Path -LiteralPath $Icon -PathType Leaf)) {
        throw "Missing extension icon $Icon"
    }
}
$Dist = Join-Path $Root "dist"
New-Item -ItemType Directory -Force -Path $Dist | Out-Null
$Zip = Join-Path $Dist "jyroxa-browser-extension.zip"
if (Test-Path $Zip) { Remove-Item $Zip -Force }
Compress-Archive -Path (Join-Path $Ext "*") -DestinationPath $Zip -Force
Write-Host "Packaged $Zip"
