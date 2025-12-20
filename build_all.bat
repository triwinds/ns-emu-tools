@echo off
setlocal enabledelayedexpansion

echo ================================
echo Building NS-EMU-TOOLS
echo ================================
echo.

REM Store the root directory
set ROOT_DIR=%~dp0

REM Step 1: Build frontend
echo [1/2] Building frontend...
cd /d "%ROOT_DIR%frontend"
if not exist package.json (
    echo Error: frontend/package.json not found!
    exit /b 1
)

call bun run build
if errorlevel 1 (
    echo Error: Frontend build failed!
    exit /b 1
)
echo Frontend build completed successfully!
echo.

REM Step 2: Build Tauri backend
echo [2/2] Building Tauri backend...
cd /d "%ROOT_DIR%src-tauri"
if not exist Cargo.toml (
    echo Error: src-tauri/Cargo.toml not found!
    exit /b 1
)

cargo build --release
if errorlevel 1 (
    echo Error: Tauri build failed!
    exit /b 1
)
echo Tauri build completed successfully!
echo.

cd /d "%ROOT_DIR%"

echo ================================
echo Build completed successfully!
echo ================================
echo.
echo Executable location: src-tauri\target\release\
pause
