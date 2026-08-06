@echo off
setlocal EnableExtensions EnableDelayedExpansion

for %%I in ("%~dp0..\..") do set "PROJECT_ROOT=%%~fI"
set "LOCK_PATH=%PROJECT_ROOT%\Cargo.lock"
set "BACKUP_PATH=%PROJECT_ROOT%\Cargo.lock.windows-backup"
set "OFFLINE_ARG="
if /I "%~1"=="--offline" set "OFFLINE_ARG=--offline"

where cargo.exe >nul 2>nul
if errorlevel 1 (
    echo ERROR: cargo.exe is required. Install the Rust MSVC toolchain from https://rustup.rs/.
    set "RC=1"
    goto :fail
)

pushd "%PROJECT_ROOT%" >nul
if errorlevel 1 (
    echo ERROR: Could not enter the repository root: %PROJECT_ROOT%
    set "RC=1"
    goto :fail
)

cargo.exe tree --package local-transcriber-windows --target x86_64-pc-windows-msvc --edges normal,build --locked !OFFLINE_ARG! >nul 2>nul
if not errorlevel 1 (
    echo Cargo.lock already matches the Windows dependency graph.
    popd
    exit /b 0
)

echo Cargo.lock is stale; regenerating it from the workspace manifests.
set "HAD_ORIGINAL_LOCK=0"
if exist "%BACKUP_PATH%" del /f /q "%BACKUP_PATH%" >nul 2>nul
if exist "%LOCK_PATH%" (
    copy /y "%LOCK_PATH%" "%BACKUP_PATH%" >nul
    if errorlevel 1 (
        echo ERROR: Could not back up Cargo.lock.
        set "RC=1"
        goto :restore
    )
    set "HAD_ORIGINAL_LOCK=1"
    del /f /q "%LOCK_PATH%" >nul
    if errorlevel 1 (
        echo ERROR: Could not remove the stale Cargo.lock.
        set "RC=1"
        goto :restore
    )
)

cargo.exe generate-lockfile !OFFLINE_ARG!
if errorlevel 1 (
    echo ERROR: Cargo could not regenerate Cargo.lock from the workspace manifests.
    set "RC=!ERRORLEVEL!"
    if "!RC!"=="0" set "RC=1"
    goto :restore
)

cargo.exe tree --package local-transcriber-windows --target x86_64-pc-windows-msvc --edges normal,build --locked !OFFLINE_ARG! >nul
if errorlevel 1 (
    echo ERROR: The regenerated Cargo.lock still does not match the Windows dependency graph.
    set "RC=!ERRORLEVEL!"
    if "!RC!"=="0" set "RC=1"
    goto :restore
)

if exist "%BACKUP_PATH%" del /f /q "%BACKUP_PATH%" >nul 2>nul
echo Cargo.lock was regenerated and now matches the Windows dependency graph.
popd
exit /b 0

:restore
if "!HAD_ORIGINAL_LOCK!"=="1" (
    if exist "%BACKUP_PATH%" move /y "%BACKUP_PATH%" "%LOCK_PATH%" >nul
) else (
    if exist "%LOCK_PATH%" del /f /q "%LOCK_PATH%" >nul 2>nul
)
if exist "%BACKUP_PATH%" del /f /q "%BACKUP_PATH%" >nul 2>nul
popd

:fail
if not defined RC set "RC=1"
if not defined LT_NO_PAUSE (
    echo.
    echo Lock resolution failed with exit code !RC!.
    echo Review the error above, then press any key to close this window.
    pause >nul
)
exit /b !RC!
