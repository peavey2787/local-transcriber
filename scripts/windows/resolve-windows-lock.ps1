[CmdletBinding()]
param(
    [switch] $Offline
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
if (Test-Path Variable:PSNativeCommandUseErrorActionPreference) {
    $PSNativeCommandUseErrorActionPreference = $false
}

$projectRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$lockPath = Join-Path $projectRoot "Cargo.lock"
$backupPath = Join-Path $projectRoot "Cargo.lock.windows-backup"

if (-not (Get-Command cargo.exe -ErrorAction SilentlyContinue)) {
    throw "cargo.exe is required. Install the Rust MSVC toolchain from https://rustup.rs/."
}

$treeArguments = @(
    "tree",
    "--package",
    "local-transcriber-windows",
    "--target",
    "x86_64-pc-windows-msvc",
    "--edges",
    "normal,build",
    "--locked"
)
if ($Offline) {
    $treeArguments += "--offline"
}

# Preserve a valid checked-in lock exactly. Regenerate only when Cargo itself
# reports that the manifests and lock no longer agree.
& cargo.exe @treeArguments *> $null
if ($LASTEXITCODE -eq 0) {
    Write-Host "Cargo.lock already matches the Windows dependency graph."
    return
}

Write-Host "Cargo.lock is stale; regenerating it from the workspace manifests"

$hadOriginalLock = Test-Path -LiteralPath $lockPath -PathType Leaf
if (Test-Path -LiteralPath $backupPath) {
    Remove-Item -LiteralPath $backupPath -Force
}
if ($hadOriginalLock) {
    Copy-Item -LiteralPath $lockPath -Destination $backupPath -Force
    Remove-Item -LiteralPath $lockPath -Force
}

$resolved = $false
try {
    $generateArguments = @("generate-lockfile")
    if ($Offline) {
        $generateArguments += "--offline"
    }

    & cargo.exe @generateArguments
    if ($LASTEXITCODE -ne 0) {
        throw "Cargo could not regenerate Cargo.lock from the workspace manifests."
    }

    & cargo.exe @treeArguments | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "The regenerated Cargo.lock still does not match the Windows dependency graph."
    }

    $resolved = $true
}
finally {
    if (-not $resolved) {
        if ($hadOriginalLock -and (Test-Path -LiteralPath $backupPath -PathType Leaf)) {
            Move-Item -LiteralPath $backupPath -Destination $lockPath -Force
        }
        elseif (-not $hadOriginalLock -and (Test-Path -LiteralPath $lockPath -PathType Leaf)) {
            Remove-Item -LiteralPath $lockPath -Force
        }
    }

    if (Test-Path -LiteralPath $backupPath) {
        Remove-Item -LiteralPath $backupPath -Force
    }
}

Write-Host "Cargo.lock was regenerated and now matches the Windows dependency graph."
