@echo off
setlocal enabledelayedexpansion
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" 1>nul 2>nul
if errorlevel 1 (
  echo vcvars64.bat failed
  exit /b 1
)
REM Drop Git's link.exe (MSYS) so MSVC's wins
set "_NEW_PATH="
for %%P in ("%PATH:;=" "%") do (
  set "P=%%~P"
  if /i not "!P!"=="D:\ProgramData\Git\usr\bin" if /i not "!P!"=="D:\ProgramData\Git\mingw64\bin" set "_NEW_PATH=!_NEW_PATH!;!P!"
)
set "PATH=%_NEW_PATH%"
set "PATH=C:\Users\yhc\.cargo\bin;%PATH%"
"C:\Users\yhc\.cargo\bin\cargo.exe" %*