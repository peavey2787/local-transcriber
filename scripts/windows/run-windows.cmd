@echo off
setlocal EnableExtensions EnableDelayedExpansion

for %%I in ("%~dp0..\..") do set "PROJECT_ROOT=%%~fI"
set "APP_DIRECTORY=%PROJECT_ROOT%\target\release"
set "BINARY=%APP_DIRECTORY%\local-stt-rs.exe"

if not exist "%BINARY%" (
    echo ERROR: The Windows release executable is missing.
    echo Run scripts\windows\install-windows.cmd first.
    set "RC=1"
    goto :fail
)
for %%F in (sherpa-onnx-c-api.dll onnxruntime.dll) do (
    if not exist "%APP_DIRECTORY%\%%F" (
        echo ERROR: %%F is missing beside the Windows release executable.
        echo Run scripts\windows\install-windows.cmd again.
        set "RC=1"
        goto :fail
    )
)

start "" /D "%APP_DIRECTORY%" "%BINARY%" %*
if errorlevel 1 (
    set "RC=!ERRORLEVEL!"
    if "!RC!"=="0" set "RC=1"
    goto :fail
)
exit /b 0

:fail
if not defined RC set "RC=1"
if not defined LT_NO_PAUSE (
    echo.
    echo Windows application failed to start with code !RC!.
    echo Review the error above, then press any key to close this window.
    pause >nul
)
exit /b !RC!
