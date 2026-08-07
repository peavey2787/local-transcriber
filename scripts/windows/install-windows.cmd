@echo off
setlocal EnableExtensions EnableDelayedExpansion

set "INSTALL_NO_PAUSE=0"

:parse_args
if "%~1"=="" goto :args_done
if /I "%~1"=="--no-pause" set "INSTALL_NO_PAUSE=1"
if /I "%~1"=="/no-pause" set "INSTALL_NO_PAUSE=1"
shift
goto :parse_args

:args_done
for %%I in ("%~dp0..\..") do set "PROJECT_ROOT=%%~fI"
set "LIBRARY_DIRECTORY=%PROJECT_ROOT%\.native\lib"
set "TARGET_DIRECTORY=%PROJECT_ROOT%\target"
set "RELEASE_DIRECTORY=%TARGET_DIRECTORY%\release"
set "BINARY=%RELEASE_DIRECTORY%\local-stt-rs.exe"

for %%C in (cargo.exe rustc.exe) do (
    where %%C >nul 2>nul
    if errorlevel 1 (
        echo ERROR: %%C is required. Install the x86_64-pc-windows-msvc Rust toolchain from https://rustup.rs/.
        set "RC=1"
        goto :fail
    )
)

set "RUST_HOST="
for /f "tokens=* delims=" %%H in ('rustc.exe -vV ^| findstr /B /C:"host:"') do set "RUST_HOST=%%H"
if /I not "!RUST_HOST!"=="host: x86_64-pc-windows-msvc" (
    echo ERROR: The x86_64-pc-windows-msvc Rust toolchain is required; found '!RUST_HOST!'.
    set "RC=1"
    goto :fail
)

pushd "%PROJECT_ROOT%" >nul
if errorlevel 1 (
    echo ERROR: Could not enter the repository root: %PROJECT_ROOT%
    set "RC=1"
    goto :fail
)

set "SAVED_NO_PAUSE=%LT_NO_PAUSE%"
set "LT_NO_PAUSE=1"
call "%~dp0resolve-windows-lock.cmd"
set "RC=!ERRORLEVEL!"
call :restore_pause_setting
if not "!RC!"=="0" goto :install_fail

set "SAVED_NO_PAUSE=%LT_NO_PAUSE%"
set "LT_NO_PAUSE=1"
call "%~dp0prepare-sherpa-runtime.cmd"
set "RC=!ERRORLEVEL!"
call :restore_pause_setting
if not "!RC!"=="0" goto :install_fail

set "SHERPA_ONNX_LIB_DIR=%LIBRARY_DIRECTORY%"
echo.
echo ==^> Building the Windows release
cargo.exe build -p local-transcriber-windows --release --locked --target-dir "%TARGET_DIRECTORY%"
if errorlevel 1 (
    set "RC=!ERRORLEVEL!"
    if "!RC!"=="0" set "RC=1"
    echo ERROR: The Windows release build failed.
    goto :install_fail
)

if not exist "%BINARY%" (
    echo ERROR: Cargo completed, but the Windows release executable was not created at:
    echo   %BINARY%
    set "RC=1"
    goto :install_fail
)

copy /y "%LIBRARY_DIRECTORY%\*.dll" "%RELEASE_DIRECTORY%\" >nul
if errorlevel 1 (
    echo ERROR: The verified Sherpa runtime DLLs could not be copied beside the executable.
    set "RC=1"
    goto :install_fail
)

popd
echo.
echo ============================================================
echo Windows installation and build completed successfully.
echo.
echo Release executable:
echo   %BINARY%
echo.
echo Next, run the application with:
echo   %~dp0run-windows.cmd
echo.
echo To create the top-level dist folder and ZIP, run:
echo   %~dp0package-windows.cmd
echo ============================================================
if "%INSTALL_NO_PAUSE%"=="0" (
    echo.
    set "INSTALL_DONE="
    set /p "INSTALL_DONE=Press Enter to close this window..."
)
exit /b 0

:restore_pause_setting
if defined SAVED_NO_PAUSE (
    set "LT_NO_PAUSE=!SAVED_NO_PAUSE!"
) else (
    set "LT_NO_PAUSE="
)
set "SAVED_NO_PAUSE="
exit /b 0

:install_fail
popd

:fail
if not defined RC set "RC=1"
if "%INSTALL_NO_PAUSE%"=="0" (
    echo.
    echo Windows installation/build failed with exit code !RC!.
    echo Review the error above, then press any key to close this window.
    pause >nul
)
exit /b !RC!
