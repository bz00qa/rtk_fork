@echo off
REM rtk installer for Windows - https://github.com/rtk-ai/rtk
REM Usage: install.cmd
REM   Builds from source using cargo install --path .
REM   Requires: Rust toolchain (rustup.rs)

setlocal enabledelayedexpansion

echo [INFO] Installing rtk from source...

where cargo >nul 2>&1
if %ERRORLEVEL% neq 0 (
    echo [ERROR] cargo not found. Install Rust from https://rustup.rs
    exit /b 1
)

cargo install --path . %*
if %ERRORLEVEL% neq 0 (
    echo [ERROR] Build failed
    exit /b 1
)

echo.
where rtk >nul 2>&1
if %ERRORLEVEL% equ 0 (
    for /f "tokens=*" %%v in ('rtk --version 2^>^&1') do echo [INFO] Installed: %%v
) else (
    echo [WARN] rtk installed but not in PATH. Add %%USERPROFILE%%\.cargo\bin to your PATH.
)

echo [INFO] Installation complete! Run 'rtk --help' to get started.
