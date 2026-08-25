@echo off
setlocal

REM Package portable zip from already-built release binary.
REM Use when the full build-portable.bat failed (e.g. NSIS download timeout)
REM but the release binary exists.

cd /d "%~dp0\.."

set EXE=src-tauri\target\release\quietly.exe
if not exist "%EXE%" (
  echo ERROR: expected binary not found at %EXE%
  exit /b 1
)

for /f "delims=" %%i in ('powershell -NoProfile -Command "Get-Date -Format 'yyyyMMdd'"') do set STAMP=%%i
set PORTABLE_DIR=dist-portable\quietly-portable-%STAMP%
set ZIP_PATH=dist-portable\quietly-portable-%STAMP%.zip

if exist dist-portable rmdir /s /q dist-portable
mkdir "%PORTABLE_DIR%"

copy /y "%EXE%" "%PORTABLE_DIR%\quietly.exe" >nul
copy /y "scripts\README.txt" "%PORTABLE_DIR%\README.txt" >nul

powershell -NoProfile -Command "Compress-Archive -Path '%PORTABLE_DIR%' -DestinationPath '%ZIP_PATH%' -Force"

echo === Package complete ===
echo   Binary:   %EXE%
echo   Portable: %ZIP_PATH%
endlocal
