@echo off
setlocal enabledelayedexpansion
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" 1>nul 2>nul
if errorlevel 1 (
  echo vcvars64.bat failed
  exit /b 1
)
REM Drop Git's link.exe (MSYS) so MSVC's wins. Match by trailing path
REM segment so this works for any Git-for-Windows install location.
set "_NEW_PATH="
for %%P in ("%PATH:;=" "%") do (
  set "P=%%~P"
  set "SKIP="
  if /i "!P:~-11!"=="Git\usr\bin"    set "SKIP=1"
  if /i "!P:~-15!"=="Git\mingw64\bin" set "SKIP=1"
  if not defined SKIP set "_NEW_PATH=!_NEW_PATH!;!P!"
)
set "PATH=%_NEW_PATH%"
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
"%USERPROFILE%\.cargo\bin\cargo.exe" %*