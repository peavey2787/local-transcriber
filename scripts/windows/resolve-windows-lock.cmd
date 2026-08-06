@echo off
setlocal
echo Resolving the Windows Cargo lockfile...
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0resolve-windows-lock.ps1" %*
set "EXIT_CODE=%ERRORLEVEL%"
if not "%EXIT_CODE%"=="0" (
    echo.
    echo Windows lock resolution failed with exit code %EXIT_CODE%.
    echo Review the error above, then press any key to close this window.
    pause >nul
)
exit /b %EXIT_CODE%
