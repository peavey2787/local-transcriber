[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$projectRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$libraryDirectory = Join-Path $projectRoot ".native\lib"

if ([System.Environment]::OSVersion.Platform -ne [System.PlatformID]::Win32NT) {
    throw "This build script supports only Windows."
}
foreach ($commandName in @("cargo.exe", "rustc.exe")) {
    if (-not (Get-Command $commandName -ErrorAction SilentlyContinue)) {
        throw "$commandName is required. Install the Rust MSVC toolchain from https://rustup.rs/."
    }
}

$hostLine = (& rustc.exe -vV | Select-String -Pattern "^host:").Line
if ($hostLine -ne "host: x86_64-pc-windows-msvc") {
    throw "The x86_64-pc-windows-msvc Rust toolchain is required; found '$hostLine'."
}

Push-Location $projectRoot
try {
    & (Join-Path $PSScriptRoot "resolve-windows-lock.ps1")

    & (Join-Path $PSScriptRoot "prepare-sherpa-runtime.ps1")
    $env:SHERPA_ONNX_LIB_DIR = $libraryDirectory

    & cargo.exe build -p local-transcriber-windows --release --locked
    if ($LASTEXITCODE -ne 0) {
        throw "The Windows release build failed."
    }
}
finally {
    Pop-Location
}

Write-Host "Windows release build ready at:"
Write-Host "  $(Join-Path $projectRoot 'target\release\local-stt-rs.exe')"
