@echo off
chcp 65001 >nul
echo === [1/3] 当前系统代理状态 ===
reg query "HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings" /v ProxyEnable
reg query "HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings" /v ProxyServer 2>nul
echo.
echo === [2/3] DB 里的 block rules ===
sqlite3 "%APPDATA%\com.quietly.app\quietly.db" "SELECT id, kind, pattern, match_type, active FROM block_rule;" 2>nul
echo.
echo === [3/3] proxy-debug.log 最后 15 行 ===
powershell -NoProfile -Command "Get-Content '%APPDATA%\com.quietly.app\proxy-debug.log' -Tail 15"
echo.
echo === 接下来请 ===
echo 1) 在 quietly 里点 Start 进入 focus 会话
echo 2) 再跑一次本脚本，确认 ProxyEnable = 0x1
echo 3) 浏览器访问 bilibili.com，应该看到 block 页
echo 4) 再跑一次脚本看 proxy 日志