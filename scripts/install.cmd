@echo off
setlocal
rem Installer per-user. Argumen diteruskan ke install.ps1, mis. -NoAutostart.
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0install.ps1" %*
if errorlevel 1 pause
endlocal