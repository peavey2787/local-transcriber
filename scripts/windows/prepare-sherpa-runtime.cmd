@echo off
setlocal EnableExtensions EnableDelayedExpansion

for %%I in ("%~dp0..\..") do set "PROJECT_ROOT=%%~fI"
set "RUNTIME_ROOT=%PROJECT_ROOT%\.native"
set "CACHE_DIRECTORY=%RUNTIME_ROOT%\cache"
set "LIBRARY_DIRECTORY=%RUNTIME_ROOT%\lib"
set "RELEASE=1.13.4"
set "ARCHIVE_NAME=sherpa-onnx-v%RELEASE%-win-x64-shared-MT-Release-lib.tar.bz2"
set "ARCHIVE_URL=https://github.com/k2-fsa/sherpa-onnx/releases/download/v%RELEASE%/%ARCHIVE_NAME%"
set "ARCHIVE_SHA256=f923e5eacb6bca83914d89cb31afa579e11eeaff9af39f8ead82ad19f44b2c9f"
set "ARCHIVE_PATH=%CACHE_DIRECTORY%\%ARCHIVE_NAME%"
set "PARTIAL_PATH=%ARCHIVE_PATH%.part"
set "RECEIPT_PATH=%RUNTIME_ROOT%\runtime.sha256"
set "ARCHIVE_ROOT=%ARCHIVE_NAME:.tar.bz2=%"
set "TEMPORARY_DIRECTORY=%RUNTIME_ROOT%\extract.!RANDOM!!RANDOM!!RANDOM!"
set "NEW_LIBRARY_DIRECTORY=%RUNTIME_ROOT%\lib.new"

set "NATIVE_ARCH=%PROCESSOR_ARCHITEW6432%"
if not defined NATIVE_ARCH set "NATIVE_ARCH=%PROCESSOR_ARCHITECTURE%"
if /I not "!NATIVE_ARCH!"=="AMD64" if /I not "!NATIVE_ARCH!"=="X86_64" (
    echo ERROR: The Sherpa runtime currently supports only Windows x64; detected '!NATIVE_ARCH!'.
    set "RC=1"
    goto :fail
)

for %%C in (curl.exe certutil.exe tar.exe) do (
    where %%C >nul 2>nul
    if errorlevel 1 (
        echo ERROR: %%C is required on Windows 10 or Windows 11.
        set "RC=1"
        goto :fail
    )
)

if not exist "%CACHE_DIRECTORY%" mkdir "%CACHE_DIRECTORY%" >nul 2>nul
if errorlevel 1 (
    echo ERROR: Could not create %CACHE_DIRECTORY%
    set "RC=1"
    goto :fail
)

if exist "%ARCHIVE_PATH%" (
    call :sha256 "%ARCHIVE_PATH%" ACTUAL_HASH
    if errorlevel 1 (
        echo WARNING: Could not hash the cached Sherpa runtime; discarding it.
        del /f /q "%ARCHIVE_PATH%" >nul 2>nul
    ) else (
        if /I not "!ACTUAL_HASH!"=="%ARCHIVE_SHA256%" (
            echo WARNING: Discarding the cached Sherpa runtime because its SHA-256 is incorrect.
            del /f /q "%ARCHIVE_PATH%" >nul 2>nul
        )
    )
)

if not exist "%ARCHIVE_PATH%" (
    if exist "%PARTIAL_PATH%" del /f /q "%PARTIAL_PATH%" >nul 2>nul
    echo Downloading the verified Sherpa/ONNX Windows runtime:
    echo   %ARCHIVE_URL%
    curl.exe --fail --location --retry 3 --retry-delay 2 --output "%PARTIAL_PATH%" "%ARCHIVE_URL%"
    if errorlevel 1 (
        echo ERROR: The Sherpa/ONNX runtime download failed.
        set "RC=!ERRORLEVEL!"
        if "!RC!"=="0" set "RC=1"
        goto :fail
    )

    call :sha256 "%PARTIAL_PATH%" ACTUAL_HASH
    if errorlevel 1 (
        echo ERROR: Could not calculate the downloaded runtime SHA-256.
        set "RC=1"
        goto :fail
    )
    if /I not "!ACTUAL_HASH!"=="%ARCHIVE_SHA256%" (
        echo ERROR: Sherpa/ONNX runtime SHA-256 verification failed.
        echo Expected: %ARCHIVE_SHA256%
        echo Actual:   !ACTUAL_HASH!
        set "RC=1"
        goto :fail
    )
    move /y "%PARTIAL_PATH%" "%ARCHIVE_PATH%" >nul
    if errorlevel 1 (
        echo ERROR: Could not activate the verified runtime archive.
        set "RC=1"
        goto :fail
    )
)

call :sha256 "%ARCHIVE_PATH%" ACTUAL_HASH
if errorlevel 1 (
    echo ERROR: Could not hash the cached Sherpa/ONNX runtime.
    set "RC=1"
    goto :fail
)
if /I not "!ACTUAL_HASH!"=="%ARCHIVE_SHA256%" (
    echo ERROR: The cached Sherpa/ONNX runtime failed SHA-256 verification.
    set "RC=1"
    goto :fail
)

if exist "%TEMPORARY_DIRECTORY%" rmdir /s /q "%TEMPORARY_DIRECTORY%" >nul 2>nul
mkdir "%TEMPORARY_DIRECTORY%" >nul 2>nul
if errorlevel 1 (
    echo ERROR: Could not create a temporary extraction directory.
    set "RC=1"
    goto :fail
)

tar.exe -xjf "%ARCHIVE_PATH%" -C "%TEMPORARY_DIRECTORY%"
if errorlevel 1 (
    echo ERROR: tar.exe could not extract the verified Sherpa/ONNX runtime.
    set "RC=!ERRORLEVEL!"
    if "!RC!"=="0" set "RC=1"
    goto :fail
)

set "EXTRACTED_LIBRARY_DIRECTORY=%TEMPORARY_DIRECTORY%\%ARCHIVE_ROOT%\lib"
for %%F in (sherpa-onnx-c-api.dll sherpa-onnx-c-api.lib onnxruntime.dll onnxruntime.lib) do (
    if not exist "%EXTRACTED_LIBRARY_DIRECTORY%\%%F" (
        echo ERROR: The verified runtime archive is missing %%F.
        set "RC=1"
        goto :fail
    )
)

if exist "%NEW_LIBRARY_DIRECTORY%" rmdir /s /q "%NEW_LIBRARY_DIRECTORY%" >nul 2>nul
move "%EXTRACTED_LIBRARY_DIRECTORY%" "%NEW_LIBRARY_DIRECTORY%" >nul
if errorlevel 1 (
    echo ERROR: Could not stage the verified runtime library directory.
    set "RC=1"
    goto :fail
)
if exist "%LIBRARY_DIRECTORY%" rmdir /s /q "%LIBRARY_DIRECTORY%" >nul 2>nul
move "%NEW_LIBRARY_DIRECTORY%" "%LIBRARY_DIRECTORY%" >nul
if errorlevel 1 (
    echo ERROR: Could not activate the verified runtime library directory.
    set "RC=1"
    goto :fail
)

> "%RECEIPT_PATH%" (
    echo archive=%ARCHIVE_NAME%
    echo sha256=%ARCHIVE_SHA256%
    echo source=%ARCHIVE_URL%
)

if exist "%TEMPORARY_DIRECTORY%" rmdir /s /q "%TEMPORARY_DIRECTORY%" >nul 2>nul
echo Verified Sherpa/ONNX runtime ready at:
echo   %LIBRARY_DIRECTORY%
echo SHA-256: %ARCHIVE_SHA256%
exit /b 0

:sha256
set "HASH_VALUE="
for /f "skip=1 tokens=* delims=" %%H in ('certutil.exe -hashfile "%~1" SHA256 2^>nul') do if not defined HASH_VALUE set "HASH_VALUE=%%H"
set "HASH_VALUE=!HASH_VALUE: =!"
if not defined HASH_VALUE exit /b 1
set "%~2=!HASH_VALUE!"
exit /b 0

:fail
if not defined RC set "RC=1"
if exist "%PARTIAL_PATH%" del /f /q "%PARTIAL_PATH%" >nul 2>nul
if exist "%TEMPORARY_DIRECTORY%" rmdir /s /q "%TEMPORARY_DIRECTORY%" >nul 2>nul
if exist "%NEW_LIBRARY_DIRECTORY%" rmdir /s /q "%NEW_LIBRARY_DIRECTORY%" >nul 2>nul
if not defined LT_NO_PAUSE (
    echo.
    echo Runtime preparation failed with exit code !RC!.
    echo Review the error above, then press any key to close this window.
    pause >nul
)
exit /b !RC!
