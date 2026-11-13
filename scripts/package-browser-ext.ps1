$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
$Ext = Join-Path $Root "extensions\browser"
$Icons = Join-Path $Ext "icons"
New-Item -ItemType Directory -Force -Path $Icons | Out-Null
Copy-Item (Join-Path $Root "assets\brand\png\jyroxa-mark-16.png") (Join-Path $Icons "icon-16.png") -Force
Copy-Item (Join-Path $Root "assets\brand\png\jyroxa-mark-32.png") (Join-Path $Icons "icon-32.png") -Force
Copy-Item (Join-Path $Root "assets\brand\png\jyroxa-mark-48.png") (Join-Path $Icons "icon-48.png") -Force
Copy-Item (Join-Path $Root "assets\brand\png\jyroxa-mark-128.png") (Join-Path $Icons "icon-128.png") -Force
$Dist = Join-Path $Root "dist"
New-Item -ItemType Directory -Force -Path $Dist | Out-Null
$Zip = Join-Path $Dist "jyroxa-browser-extension.zip"
if (Test-Path $Zip) { Remove-Item $Zip -Force }
Compress-Archive -Path (Join-Path $Ext "*") -DestinationPath $Zip -Force
Write-Host "Packaged $Zip"
