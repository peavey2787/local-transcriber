[CmdletBinding()]
param(
    [switch] $Offline
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$projectRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$lockPath = Join-Path $projectRoot "Cargo.lock"
$backupPath = Join-Path $projectRoot "Cargo.lock.windows-backup"

function Invoke-CargoProcess {
    param(
        [Parameter(Mandatory)]
        [string[]] $Arguments,
        [switch] $Quiet
    )

    $stdoutPath = Join-Path ([System.IO.Path]::GetTempPath()) ("local-transcriber-cargo-{0}.stdout" -f [guid]::NewGuid().ToString("N"))
    $stderrPath = Join-Path ([System.IO.Path]::GetTempPath()) ("local-transcriber-cargo-{0}.stderr" -f [guid]::NewGuid().ToString("N"))

    try {
        $process = Start-Process `
            -FilePath "cargo.exe" `
            -ArgumentList $Arguments `
            -WorkingDirectory $projectRoot `
            -NoNewWindow `
            -Wait `
            -PassThru `
            -RedirectStandardOutput $stdoutPath `
            -RedirectStandardError $stderrPath

        $stdout = if (Test-Path -LiteralPath $stdoutPath) {
            Get-Content -LiteralPath $stdoutPath -Raw -ErrorAction SilentlyContinue
        }
        else {
            ""
        }
        $stderr = if (Test-Path -LiteralPath $stderrPath) {
            Get-Content -LiteralPath $stderrPath -Raw -ErrorAction SilentlyContinue
        }
        else {
            ""
        }

        if (-not $Quiet) {
            if (-not [System.String]::IsNullOrWhiteSpace($stdout)) {
                Write-Host ($stdout.TrimEnd())
            }
            if (-not [System.String]::IsNullOrWhiteSpace($stderr)) {
                Write-Host ($stderr.TrimEnd())
            }
        }

        return [int] $process.ExitCode
    }
    finally {
        Remove-Item -LiteralPath $stdoutPath, $stderrPath -Force -ErrorAction SilentlyContinue
    }
}

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

# Do not invoke Cargo directly with stderr redirected into PowerShell's error
# stream. Windows PowerShell 5.1 can turn Cargo's expected nonzero result into
# a terminating NativeCommandError before $LASTEXITCODE can be inspected.
$treeExitCode = Invoke-CargoProcess -Arguments $treeArguments -Quiet
if ($treeExitCode -eq 0) {
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

    $generateExitCode = Invoke-CargoProcess -Arguments $generateArguments
    if ($generateExitCode -ne 0) {
        throw "Cargo could not regenerate Cargo.lock from the workspace manifests."
    }

    $verifyExitCode = Invoke-CargoProcess -Arguments $treeArguments
    if ($verifyExitCode -ne 0) {
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
