@echo off
setlocal
rem Uninstaller per-user. Argumen diteruskan ke uninstall.ps1, mis. -PurgeConfig.
rem CWD dipindah keluar folder instalasi karena folder itu ikut dihapus.
cd /d "%TEMP%"
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0uninstall.ps1" %*
if errorlevel 1 pause
endlocal