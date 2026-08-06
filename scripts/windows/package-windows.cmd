@echo off
setlocal
echo Starting Windows packaging...
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0package-windows.ps1" %*
set "EXIT_CODE=%ERRORLEVEL%"
if not "%EXIT_CODE%"=="0" (
    echo.
    echo Windows packaging failed with exit code %EXIT_CODE%.
    echo Review the error above, then press any key to close this window.
    pause >nul
)
exit /b %EXIT_CODE%
