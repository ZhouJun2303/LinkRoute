param(
    [string] $Version = "2.2.2",
    [string] $ArchiveSuffix = "A",
    [string] $ExpectedSha256 = "63CB41763BB4B20F600B6DE04E991A9C2BE73279E317D4D82F237B150C5F3F15",
    [string] $OutputDir = ""
)

$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$vendorDir = Join-Path $root "vendor\windivert"
$archiveName = "WinDivert-$Version-$ArchiveSuffix.zip"
$archivePath = Join-Path $vendorDir $archiveName
$extractDir = Join-Path $vendorDir "WinDivert-$Version-$ArchiveSuffix"
$url = "https://github.com/basil00/WinDivert/releases/download/v$Version/$archiveName"

New-Item -ItemType Directory -Force -Path $vendorDir | Out-Null

if (-not (Test-Path -LiteralPath $archivePath)) {
    Invoke-WebRequest -Uri $url -OutFile $archivePath
}

$hash = (Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash.ToUpperInvariant()
if ($hash -ne $ExpectedSha256.ToUpperInvariant()) {
    throw "WinDivert archive SHA-256 mismatch. Expected $ExpectedSha256, got $hash"
}

if (-not (Test-Path -LiteralPath $extractDir)) {
    Expand-Archive -LiteralPath $archivePath -DestinationPath $vendorDir -Force
}

$sourceDir = Join-Path $extractDir "x64"
$dll = Join-Path $sourceDir "WinDivert.dll"
$driver = Join-Path $sourceDir "WinDivert64.sys"
if (-not (Test-Path -LiteralPath $dll)) { throw "missing $dll" }
if (-not (Test-Path -LiteralPath $driver)) { throw "missing $driver" }

if ($OutputDir) {
    New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
    Copy-Item -LiteralPath $dll -Destination (Join-Path $OutputDir "WinDivert.dll") -Force
    Copy-Item -LiteralPath $driver -Destination (Join-Path $OutputDir "WinDivert64.sys") -Force
}

Write-Host "WinDivert $Version ready at $sourceDir"
