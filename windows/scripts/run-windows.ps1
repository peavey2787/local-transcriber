[CmdletBinding()]
param(
    [Parameter(ValueFromRemainingArguments)]
    [string[]] $ApplicationArguments
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$projectRoot = Split-Path -Parent $PSScriptRoot
$releaseDirectory = Join-Path $projectRoot "target\release"
$binary = Join-Path $releaseDirectory "local-stt-rs.exe"

if ([System.Environment]::OSVersion.Platform -ne [System.PlatformID]::Win32NT) {
    throw "This launcher supports only Windows."
}
if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) {
    throw "The release binary is missing. Run .\scripts\build-windows.ps1 first."
}
foreach ($fileName in @("sherpa-onnx-c-api.dll", "onnxruntime.dll")) {
    if (-not (Test-Path -LiteralPath (Join-Path $releaseDirectory $fileName) -PathType Leaf)) {
        throw "$fileName is missing beside the release binary. Rebuild the application."
    }
}

& $binary @ApplicationArguments
exit $LASTEXITCODE
