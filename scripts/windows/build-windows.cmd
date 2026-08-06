@echo off
setlocal
echo Starting the Windows release build...
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0build-windows.ps1" %*
set "EXIT_CODE=%ERRORLEVEL%"
if not "%EXIT_CODE%"=="0" (
    echo.
    echo Windows build failed with exit code %EXIT_CODE%.
    echo Review the error above, then press any key to close this window.
    pause >nul
)
exit /b %EXIT_CODE%
