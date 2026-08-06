@echo off
setlocal EnableExtensions EnableDelayedExpansion

for %%I in ("%~dp0..\..") do set "PROJECT_ROOT=%%~fI"
set "LIBRARY_DIRECTORY=%PROJECT_ROOT%\.native\lib"
set "RELEASE_DIRECTORY=%PROJECT_ROOT%\target\release"

for %%C in (cargo.exe rustfmt.exe cargo-clippy.exe) do (
    where %%C >nul 2>nul
    if errorlevel 1 (
        echo ERROR: %%C is required by the Windows quality gate.
        set "RC=1"
        goto :fail
    )
)

pushd "%PROJECT_ROOT%" >nul
if errorlevel 1 (
    echo ERROR: Could not enter the repository root: %PROJECT_ROOT%
    set "RC=1"
    goto :fail
)

set "SAVED_NO_PAUSE=%LT_NO_PAUSE%"
set "LT_NO_PAUSE=1"
call "%~dp0prepare-sherpa-runtime.cmd"
set "RC=!ERRORLEVEL!"
call :restore_pause_setting
if not "!RC!"=="0" goto :audit_fail

set "SHERPA_ONNX_LIB_DIR=%LIBRARY_DIRECTORY%"
set "CARGO_NET_OFFLINE=true"

echo.
echo ==^> Formatting check
cargo.exe fmt --all -- --check
if errorlevel 1 goto :cargo_fail

set "SAVED_NO_PAUSE=%LT_NO_PAUSE%"
set "LT_NO_PAUSE=1"
call "%~dp0resolve-windows-lock.cmd" --offline
set "RC=!ERRORLEVEL!"
call :restore_pause_setting
if not "!RC!"=="0" goto :audit_fail

echo.
echo ==^> Clippy with warnings denied
cargo.exe clippy -p transcriber-core -p transcriber-ui -p local-transcriber-windows --all-targets --locked --offline -- -D warnings
if errorlevel 1 goto :cargo_fail

echo.
echo ==^> Unit tests
cargo.exe test -p transcriber-core -p transcriber-ui -p local-transcriber-windows --all-targets --locked --offline
if errorlevel 1 goto :cargo_fail

echo.
echo ==^> Release build
cargo.exe build -p local-transcriber-windows --release --locked --offline
if errorlevel 1 goto :cargo_fail

copy /y "%LIBRARY_DIRECTORY%\*.dll" "%RELEASE_DIRECTORY%\" >nul
if errorlevel 1 (
    echo ERROR: The verified Sherpa runtime DLLs could not be copied beside the executable.
    set "RC=1"
    goto :audit_fail
)

popd
echo.
echo All Windows code-audit gates passed.
exit /b 0

:cargo_fail
set "RC=!ERRORLEVEL!"
if "!RC!"=="0" set "RC=1"

:audit_fail
popd

:fail
if not defined RC set "RC=1"
if not defined LT_NO_PAUSE (
    echo.
    echo Windows audit failed with exit code !RC!.
    echo Review the error above, then press any key to close this window.
    pause >nul
)
exit /b !RC!

:restore_pause_setting
if defined SAVED_NO_PAUSE (
    set "LT_NO_PAUSE=!SAVED_NO_PAUSE!"
) else (
    set "LT_NO_PAUSE="
)
set "SAVED_NO_PAUSE="
exit /b 0
