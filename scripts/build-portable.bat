@echo off
setlocal

REM Build the Tauri app in release mode and package a portable ZIP.
REM Usage: scripts\build-portable.bat
REM Output:
REM   src-tauri\target\release\quietly.exe           (standalone binary)
REM   src-tauri\target\release\bundle\nsis\*.exe     (NSIS installer)
REM   dist-portable\quietly-portable-YYYYMMDD.zip    (portable ZIP)

echo === Quietly portable build ===

cd /d "%~dp0\.."

echo.
echo [1/4] Running pnpm tauri build ...
call pnpm tauri build
if errorlevel 1 (
  echo ERROR: tauri build failed.
  exit /b 1
)

set EXE=src-tauri\target\release\quietly.exe
if not exist "%EXE%" (
  echo ERROR: expected binary not found at %EXE%
  exit /b 1
)

set STAMP=%date:~-4%%date:~3,2%%date:~0,2%
set PORTABLE_DIR=dist-portable\quietly-portable-%STAMP%
set ZIP_PATH=dist-portable\quietly-portable-%STAMP%.zip

echo.
echo [2/4] Preparing portable folder %PORTABLE_DIR% ...
if exist dist-portable rmdir /s /q dist-portable
mkdir "%PORTABLE_DIR%"

echo.
echo [3/4] Copying artifacts ...
copy /y "%EXE%" "%PORTABLE_DIR%\quietly.exe" >nul

(
  echo Quietly - Portable Edition
  echo ===========================
  echo.
  echo Run quietly.exe to start. No installation required.
  echo.
  echo System requirements:
  echo   - Windows 10 1809+ or Windows 11
  echo   - Microsoft Edge WebView2 Runtime (preinstalled on most systems)
  echo.
  echo Data is stored in %%APPDATA%%\com.quietly.app\
) > "%PORTABLE_DIR%\README.txt"

echo.
echo [4/4] Creating ZIP %ZIP_PATH% ...
powershell -NoProfile -Command "Compress-Archive -Path '%PORTABLE_DIR%' -DestinationPath '%ZIP_PATH%' -Force"

echo.
echo === Build complete ===
echo   Binary:   %EXE%
echo   Portable: %ZIP_PATH%
endlocal
