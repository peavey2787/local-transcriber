[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$projectRoot = Split-Path -Parent $PSScriptRoot
$libraryDirectory = Join-Path $projectRoot ".native\lib"

function Invoke-Checked {
    param(
        [Parameter(Mandatory)]
        [string] $Command,
        [Parameter(Mandatory)]
        [string[]] $Arguments,
        [switch] $DiscardOutput
    )

    if ($DiscardOutput) {
        & $Command @Arguments | Out-Null
    }
    else {
        & $Command @Arguments
    }
    if ($LASTEXITCODE -ne 0) {
        throw "$Command failed with exit code $LASTEXITCODE."
    }
}

foreach ($commandName in @("cargo.exe", "rustfmt.exe", "cargo-clippy.exe")) {
    if (-not (Get-Command $commandName -ErrorAction SilentlyContinue)) {
        throw "$commandName is required by the Windows quality gate."
    }
}

Push-Location $projectRoot
try {
    & (Join-Path $PSScriptRoot "prepare-sherpa-runtime.ps1")
    $env:SHERPA_ONNX_LIB_DIR = $libraryDirectory
    $env:CARGO_NET_OFFLINE = "true"

    Write-Host "`n==> Formatting check"
    Invoke-Checked cargo.exe @("fmt", "--all", "--", "--check")

    Write-Host "`n==> Locked dependency metadata"
    Invoke-Checked cargo.exe @(
        "metadata",
        "--locked",
        "--offline",
        "--no-deps",
        "--format-version",
        "1"
    ) -DiscardOutput

    Write-Host "`n==> Clippy with warnings denied"
    Invoke-Checked cargo.exe @("clippy", "--all-targets", "--locked", "--offline", "--", "-D", "warnings")

    Write-Host "`n==> Unit tests"
    Invoke-Checked cargo.exe @("test", "--all-targets", "--locked", "--offline")

    Write-Host "`n==> Release build"
    Invoke-Checked cargo.exe @("build", "--release", "--locked", "--offline")
}
finally {
    Pop-Location
}

Write-Host "`nAll Windows code-audit gates passed."
