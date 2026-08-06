@echo off
setlocal EnableExtensions EnableDelayedExpansion

for %%I in ("%~dp0..\..") do set "PROJECT_ROOT=%%~fI"
set "RELEASE_DIRECTORY=%PROJECT_ROOT%\target\release"
set "LIBRARY_DIRECTORY=%PROJECT_ROOT%\.native\lib"
set "RUNTIME_RECEIPT=%PROJECT_ROOT%\.native\runtime.sha256"
set "DISTRIBUTION_DIRECTORY=%PROJECT_ROOT%\dist"
set "PACKAGE_NAME=local-stt-windows-x64"
set "PACKAGE_DIRECTORY=%DISTRIBUTION_DIRECTORY%\%PACKAGE_NAME%"
set "ARCHIVE_PATH=%DISTRIBUTION_DIRECTORY%\%PACKAGE_NAME%.zip"
set "STAGING_ROOT=%DISTRIBUTION_DIRECTORY%\.%PACKAGE_NAME%-staging"
set "STAGING_PACKAGE_DIRECTORY=%STAGING_ROOT%\%PACKAGE_NAME%"

where tar.exe >nul 2>nul
if errorlevel 1 (
    echo ERROR: tar.exe is required to create the Windows package.
    set "RC=1"
    goto :fail
)
where certutil.exe >nul 2>nul
if errorlevel 1 (
    echo ERROR: certutil.exe is required to hash the Windows package.
    set "RC=1"
    goto :fail
)

set "SAVED_NO_PAUSE=%LT_NO_PAUSE%"
set "LT_NO_PAUSE=1"
call "%~dp0build-windows.cmd"
set "RC=!ERRORLEVEL!"
if defined SAVED_NO_PAUSE (
    set "LT_NO_PAUSE=!SAVED_NO_PAUSE!"
) else (
    set "LT_NO_PAUSE="
)
set "SAVED_NO_PAUSE="
if not "!RC!"=="0" goto :fail

set "BINARY=%RELEASE_DIRECTORY%\local-stt-rs.exe"
if not exist "%BINARY%" (
    echo ERROR: The Windows release binary is missing after the build.
    set "RC=1"
    goto :fail
)
if not exist "%RUNTIME_RECEIPT%" (
    echo ERROR: The verified native-runtime receipt is missing.
    set "RC=1"
    goto :fail
)

if exist "%STAGING_ROOT%" rmdir /s /q "%STAGING_ROOT%" >nul 2>nul
mkdir "%STAGING_PACKAGE_DIRECTORY%" >nul 2>nul
if errorlevel 1 (
    echo ERROR: Could not create the package staging directory.
    set "RC=1"
    goto :fail
)

copy /y "%BINARY%" "%STAGING_PACKAGE_DIRECTORY%\local-stt.exe" >nul
if errorlevel 1 goto :copy_fail
copy /y "%LIBRARY_DIRECTORY%\*.dll" "%STAGING_PACKAGE_DIRECTORY%\" >nul
if errorlevel 1 goto :copy_fail
copy /y "%RUNTIME_RECEIPT%" "%STAGING_PACKAGE_DIRECTORY%\native-runtime.sha256" >nul
if errorlevel 1 goto :copy_fail
copy /y "%PROJECT_ROOT%\README.md" "%STAGING_PACKAGE_DIRECTORY%\" >nul
if errorlevel 1 goto :copy_fail
copy /y "%PROJECT_ROOT%\apps\windows\SECURITY.md" "%STAGING_PACKAGE_DIRECTORY%\" >nul
if errorlevel 1 goto :copy_fail

if exist "%PACKAGE_DIRECTORY%" rmdir /s /q "%PACKAGE_DIRECTORY%" >nul 2>nul
mkdir "%PACKAGE_DIRECTORY%" >nul 2>nul
xcopy "%STAGING_PACKAGE_DIRECTORY%\*" "%PACKAGE_DIRECTORY%\" /E /I /Y >nul
if errorlevel 1 (
    echo ERROR: Could not populate the unpacked distribution directory.
    set "RC=1"
    goto :cleanup_fail
)

if exist "%ARCHIVE_PATH%" del /f /q "%ARCHIVE_PATH%" >nul 2>nul
tar.exe -a -cf "%ARCHIVE_PATH%" -C "%STAGING_ROOT%" "%PACKAGE_NAME%"
if errorlevel 1 (
    echo ERROR: Could not create %ARCHIVE_PATH%
    set "RC=!ERRORLEVEL!"
    if "!RC!"=="0" set "RC=1"
    goto :cleanup_fail
)

call :sha256 "%ARCHIVE_PATH%" ARCHIVE_HASH
if errorlevel 1 (
    echo ERROR: Could not calculate the package SHA-256.
    set "RC=1"
    goto :cleanup_fail
)

if exist "%STAGING_ROOT%" rmdir /s /q "%STAGING_ROOT%" >nul 2>nul
echo Packed: %ARCHIVE_PATH%
echo SHA-256: !ARCHIVE_HASH!
exit /b 0

:copy_fail
echo ERROR: Could not copy all required files into the package.
set "RC=1"

:cleanup_fail
if exist "%STAGING_ROOT%" rmdir /s /q "%STAGING_ROOT%" >nul 2>nul

goto :fail

:sha256
set "HASH_VALUE="
for /f "skip=1 tokens=* delims=" %%H in ('certutil.exe -hashfile "%~1" SHA256 2^>nul') do if not defined HASH_VALUE set "HASH_VALUE=%%H"
set "HASH_VALUE=!HASH_VALUE: =!"
if not defined HASH_VALUE exit /b 1
set "%~2=!HASH_VALUE!"
exit /b 0

:fail
if not defined RC set "RC=1"
if not defined LT_NO_PAUSE (
    echo.
    echo Windows packaging failed with exit code !RC!.
    echo Review the error above, then press any key to close this window.
    pause >nul
)
exit /b !RC!
