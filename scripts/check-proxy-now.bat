@echo off
chcp 65001 >nul
echo === ProxyEnable ===
reg query "HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings" /v ProxyEnable
echo === ProxyServer ===
reg query "HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings" /v ProxyServer
echo === Marker file ===
if exist "%APPDATA%\com.quietly.app\proxy-marker.json" (
    type "%APPDATA%\com.quietly.app\proxy-marker.json"
) else (
    echo No marker file
)