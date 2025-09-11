@echo off
REM Wrapper for Update-WeylandTavern.ps1 using default arguments

REM Go to directory
cd /d "%~dp0"

REM Call PowerShell script with default args
pwsh.exe -NoProfile -ExecutionPolicy Bypass -File ".\tools\Update-WeylandTavern.ps1" -Ref origin/nightly -PinExact

pause
