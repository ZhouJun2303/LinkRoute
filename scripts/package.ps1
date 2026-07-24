$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$dist = Join-Path $root "dist"
$releaseDir = Join-Path $root "target\release"
New-Item -ItemType Directory -Force -Path $dist | Out-Null

& (Join-Path $PSScriptRoot "fetch-windivert.ps1") -OutputDir $releaseDir

$repository = if ($env:GITHUB_REPOSITORY) { $env:GITHUB_REPOSITORY } else { "ZhouJun2303/LinkRoute" }
$tag = if ($env:GITHUB_REF_NAME) { $env:GITHUB_REF_NAME } else { "v0.1.0" }
$zeroSignature = "00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"

Copy-Item -LiteralPath (Join-Path $root "README.md") -Destination (Join-Path $dist "README.md") -Force
Copy-Item -LiteralPath (Join-Path $root "LICENSE") -Destination (Join-Path $dist "LICENSE") -Force
Copy-Item -LiteralPath (Join-Path $root "THIRD_PARTY_NOTICES.md") -Destination (Join-Path $dist "THIRD_PARTY_NOTICES.md") -Force
Copy-Item -LiteralPath (Join-Path $releaseDir "WinDivert.dll") -Destination (Join-Path $dist "WinDivert.dll") -Force
Copy-Item -LiteralPath (Join-Path $releaseDir "WinDivert64.sys") -Destination (Join-Path $dist "WinDivert64.sys") -Force

$installer = Join-Path $root "installer\output\LinkRouteSetup-x64.exe"
if (-not (Test-Path -LiteralPath $installer)) {
    & (Join-Path $PSScriptRoot "build-installer.ps1")
}
if (-not (Test-Path -LiteralPath $installer)) {
    throw "Installer was not produced at $installer"
}

$distInstaller = Join-Path $dist "LinkRouteSetup-x64.exe"
Copy-Item -LiteralPath $installer -Destination $distInstaller -Force
$setupHash = (Get-FileHash -LiteralPath $distInstaller -Algorithm SHA256).Hash.ToLowerInvariant()
Set-Content -LiteralPath (Join-Path $dist "LinkRouteSetup-x64.exe.sha256") -Value "$setupHash  LinkRouteSetup-x64.exe" -Encoding ASCII

$manifest = @"
version = "$tag"
url = "https://github.com/$repository/releases/download/$tag/LinkRouteSetup-x64.exe"
sha256 = "$setupHash"
signature = "$zeroSignature"
"@

Set-Content -LiteralPath (Join-Path $dist "update-manifest.txt") -Value $manifest -Encoding ASCII
Set-Content -LiteralPath (Join-Path $dist "update-manifest.txt.sig") -Value $zeroSignature -Encoding ASCII
