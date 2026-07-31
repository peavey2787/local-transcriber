[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$projectRoot = Split-Path -Parent $PSScriptRoot
$releaseDirectory = Join-Path $projectRoot "target\release"
$libraryDirectory = Join-Path $projectRoot ".native\lib"
$runtimeReceipt = Join-Path $projectRoot ".native\runtime.sha256"
$distributionDirectory = Join-Path $projectRoot "dist"
$packageName = "local-stt-windows-x64"
$packageDirectory = Join-Path $distributionDirectory $packageName
$archivePath = Join-Path $distributionDirectory "$packageName.zip"
$stagingRoot = Join-Path $distributionDirectory ".$packageName-staging"
$stagingPackageDirectory = Join-Path $stagingRoot $packageName

Push-Location $projectRoot
try {
    & (Join-Path $PSScriptRoot "build-windows.ps1")

    $binary = Join-Path $releaseDirectory "local-stt-rs.exe"
    if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) {
        throw "The Windows release binary is missing after the build."
    }
    if (-not (Test-Path -LiteralPath $runtimeReceipt -PathType Leaf)) {
        throw "The verified native-runtime receipt is missing."
    }

    if (Test-Path -LiteralPath $stagingRoot) {
        Remove-Item -LiteralPath $stagingRoot -Recurse -Force
    }
    New-Item -ItemType Directory -Path $stagingPackageDirectory -Force | Out-Null

    Copy-Item -LiteralPath $binary -Destination (Join-Path $stagingPackageDirectory "local-stt.exe")
    Get-ChildItem -LiteralPath $libraryDirectory -Filter "*.dll" -File |
        Copy-Item -Destination $stagingPackageDirectory
    Copy-Item -LiteralPath $runtimeReceipt -Destination (Join-Path $stagingPackageDirectory "native-runtime.sha256")
    Copy-Item -LiteralPath (Join-Path $projectRoot "README.md") -Destination $stagingPackageDirectory
    Copy-Item -LiteralPath (Join-Path $projectRoot "SECURITY.md") -Destination $stagingPackageDirectory

    New-Item -ItemType Directory -Path $packageDirectory -Force | Out-Null
    Get-ChildItem -LiteralPath $packageDirectory -Force |
        Remove-Item -Recurse -Force
    Get-ChildItem -LiteralPath $stagingPackageDirectory -Force |
        Copy-Item -Destination $packageDirectory -Recurse

    if (Test-Path -LiteralPath $archivePath) {
        Remove-Item -LiteralPath $archivePath -Force
    }
    Compress-Archive -LiteralPath $stagingPackageDirectory -DestinationPath $archivePath -CompressionLevel Optimal
}
finally {
    if (Test-Path -LiteralPath $stagingRoot) {
        Remove-Item -LiteralPath $stagingRoot -Recurse -Force
    }
    Pop-Location
}

$digest = (Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash.ToLowerInvariant()
Write-Host "Packed: $archivePath"
Write-Host "SHA-256: $digest"
