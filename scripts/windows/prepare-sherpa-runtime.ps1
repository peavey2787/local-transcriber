[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$projectRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$runtimeRoot = Join-Path $projectRoot ".native"
$cacheDirectory = Join-Path $runtimeRoot "cache"
$libraryDirectory = Join-Path $runtimeRoot "lib"
$release = "1.13.4"
$archiveName = "sherpa-onnx-v$release-win-x64-shared-MT-Release-lib.tar.bz2"
$archiveUrl = "https://github.com/k2-fsa/sherpa-onnx/releases/download/v$release/$archiveName"
$archiveSha256 = "f923e5eacb6bca83914d89cb31afa579e11eeaff9af39f8ead82ad19f44b2c9f"
$archivePath = Join-Path $cacheDirectory $archiveName
$partialPath = "$archivePath.part"
$receiptPath = Join-Path $runtimeRoot "runtime.sha256"

function Get-NativeWindowsArchitecture {
    $runtimeInformationType = [System.Runtime.InteropServices.RuntimeInformation]
    $architectureProperty = $runtimeInformationType.GetProperty("OSArchitecture")
    if ($null -ne $architectureProperty) {
        $architecture = $architectureProperty.GetValue($null, $null)
        if ($null -ne $architecture) {
            return $architecture.ToString().ToUpperInvariant()
        }
    }

    $architectureName = [System.Environment]::GetEnvironmentVariable("PROCESSOR_ARCHITEW6432")
    if ([System.String]::IsNullOrWhiteSpace($architectureName)) {
        $architectureName = [System.Environment]::GetEnvironmentVariable("PROCESSOR_ARCHITECTURE")
    }
    if ([System.String]::IsNullOrWhiteSpace($architectureName)) {
        return "UNKNOWN"
    }

    switch ($architectureName.Trim().ToUpperInvariant()) {
        "AMD64" { return "X64" }
        "X86_64" { return "X64" }
        "ARM64" { return "ARM64" }
        "X86" { return "X86" }
        "IA64" { return "IA64" }
        default { return "UNKNOWN" }
    }
}

function Assert-WindowsX64 {
    if ([System.Environment]::OSVersion.Platform -ne [System.PlatformID]::Win32NT) {
        throw "This runtime preparation script supports only Windows."
    }

    $architecture = Get-NativeWindowsArchitecture
    if ($architecture -ne "X64") {
        throw "This runtime preparation script currently supports only Windows x64; detected '$architecture'."
    }
}

function Test-Archive {
    if (-not (Test-Path -LiteralPath $archivePath -PathType Leaf)) {
        return $false
    }
    $actual = (Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash
    return $actual.Equals($archiveSha256, [System.StringComparison]::OrdinalIgnoreCase)
}

Assert-WindowsX64
if (-not (Get-Command tar.exe -ErrorAction SilentlyContinue)) {
    throw "tar.exe is required. Use a supported Windows 10 or Windows 11 installation."
}

New-Item -ItemType Directory -Path $cacheDirectory -Force | Out-Null

if ((Test-Path -LiteralPath $archivePath) -and -not (Test-Archive)) {
    Write-Warning "Discarding the Sherpa runtime archive because its SHA-256 is incorrect."
    Remove-Item -LiteralPath $archivePath -Force
}

if (-not (Test-Path -LiteralPath $archivePath)) {
    if (Test-Path -LiteralPath $partialPath) {
        Remove-Item -LiteralPath $partialPath -Force
    }

    Write-Host "Downloading the verified Sherpa/ONNX Windows runtime:"
    Write-Host "  $archiveUrl"
    [System.Net.ServicePointManager]::SecurityProtocol = [System.Net.SecurityProtocolType]::Tls12
    try {
        Invoke-WebRequest -Uri $archiveUrl -OutFile $partialPath
        $actual = (Get-FileHash -LiteralPath $partialPath -Algorithm SHA256).Hash
        if (-not $actual.Equals($archiveSha256, [System.StringComparison]::OrdinalIgnoreCase)) {
            throw "Sherpa/ONNX runtime SHA-256 verification failed."
        }
        Move-Item -LiteralPath $partialPath -Destination $archivePath
    }
    catch {
        if (Test-Path -LiteralPath $partialPath) {
            Remove-Item -LiteralPath $partialPath -Force
        }
        throw
    }
}

if (-not (Test-Archive)) {
    throw "The cached Sherpa/ONNX runtime failed SHA-256 verification."
}

$temporaryDirectory = Join-Path $runtimeRoot "extract.$([guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Path $temporaryDirectory -Force | Out-Null

try {
    & tar.exe -xjf $archivePath -C $temporaryDirectory
    if ($LASTEXITCODE -ne 0) {
        throw "tar.exe could not extract the verified Sherpa/ONNX runtime."
    }

    $archiveRootName = $archiveName -replace "\.tar\.bz2$", ""
    $extractedLibraryDirectory = Join-Path (Join-Path $temporaryDirectory $archiveRootName) "lib"
    $requiredFiles = @(
        "sherpa-onnx-c-api.dll",
        "sherpa-onnx-c-api.lib",
        "onnxruntime.dll",
        "onnxruntime.lib"
    )
    foreach ($fileName in $requiredFiles) {
        $requiredPath = Join-Path $extractedLibraryDirectory $fileName
        if (-not (Test-Path -LiteralPath $requiredPath -PathType Leaf)) {
            throw "The verified runtime archive is missing $fileName."
        }
    }

    $newLibraryDirectory = Join-Path $runtimeRoot "lib.new"
    if (Test-Path -LiteralPath $newLibraryDirectory) {
        Remove-Item -LiteralPath $newLibraryDirectory -Recurse -Force
    }
    Move-Item -LiteralPath $extractedLibraryDirectory -Destination $newLibraryDirectory
    if (Test-Path -LiteralPath $libraryDirectory) {
        Remove-Item -LiteralPath $libraryDirectory -Recurse -Force
    }
    Move-Item -LiteralPath $newLibraryDirectory -Destination $libraryDirectory
}
finally {
    if (Test-Path -LiteralPath $temporaryDirectory) {
        Remove-Item -LiteralPath $temporaryDirectory -Recurse -Force
    }
}

@(
    "archive=$archiveName"
    "sha256=$archiveSha256"
    "source=$archiveUrl"
) | Set-Content -LiteralPath $receiptPath -Encoding utf8

Write-Host "Verified Sherpa/ONNX runtime ready at:"
Write-Host "  $libraryDirectory"
Write-Host "SHA-256: $archiveSha256"
