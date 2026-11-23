@echo off
REM ==============================================================================
REM 🌌 NeXXUs Sovereign Node — One-Click Windows Daemon Launcher
REM ==============================================================================

setlocal
title NeXXUs Sovereign Node Daemon

echo ==========================================================
echo 🌌 Starting NeXXUs Sovereign Node Daemon...
echo ==========================================================
echo.

where nexxusd >nul 2>nul
if %errorlevel% equ 0 (
    echo [*] Launching global nexxusd daemon...
    nexxusd serve
) else (
    REM Check if local target binary exists
    if exist "%~dp0target\release\nexxusd.exe" (
        echo [*] Running from target\release\nexxusd.exe...
        "%~dp0target\release\nexxusd.exe" serve
    ) else if exist "%~dp0dist\nexxusd.exe" (
        echo [*] Running from dist\nexxusd.exe...
        "%~dp0dist\nexxusd.exe" serve
    ) else (
        echo [ERROR] NeXXUs is not installed yet.
        echo Please run install.bat first!
        pause
        exit /b 1
    )
)

pause
