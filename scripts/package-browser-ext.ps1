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
$Stage = Join-Path $env:TEMP "jyroxa-extension-$([guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Force -Path $Stage | Out-Null
try {
    # Never recurse through the development node_modules junction or package tests.
    $RuntimeFiles = @(
        'background.js', 'content.js', 'context-menus.js', 'handoff-dedupe.js',
        'intercept.js', 'manifest.json', 'media-monitor.js', 'native.js',
        'popup.css', 'popup.html', 'popup.js', 'video-grab.css', 'video-grab.js'
    )
    foreach ($Name in $RuntimeFiles) {
        $Source = Join-Path $Ext $Name
        if (-not (Test-Path -LiteralPath $Source -PathType Leaf)) {
            throw "Missing extension runtime file $Source"
        }
        Copy-Item -LiteralPath $Source -Destination (Join-Path $Stage $Name)
    }
    Copy-Item -LiteralPath $Icons -Destination (Join-Path $Stage 'icons') -Recurse
    if (Test-Path -LiteralPath $Zip) { Remove-Item -LiteralPath $Zip -Force }
    Compress-Archive -Path (Join-Path $Stage "*") -DestinationPath $Zip -Force
    Write-Host "Packaged $Zip"
} finally {
    $ResolvedStage = [System.IO.Path]::GetFullPath($Stage)
    $ResolvedTemp = [System.IO.Path]::GetFullPath($env:TEMP).TrimEnd('\') + '\'
    if (-not $ResolvedStage.StartsWith($ResolvedTemp, [System.StringComparison]::OrdinalIgnoreCase) -or
        -not ([System.IO.Path]::GetFileName($ResolvedStage)).StartsWith('jyroxa-extension-')) {
        throw "Refusing to remove an unexpected extension staging directory: $ResolvedStage"
    }
    Remove-Item -LiteralPath $ResolvedStage -Recurse -Force -ErrorAction SilentlyContinue
}
