@echo off
setlocal
cd /d "%~dp0"

where cargo >nul 2>nul
if errorlevel 1 (
    echo Error: Cargo was not found in PATH. Install Rust from https://rustup.rs/ and try again.
    exit /b 1
)

echo Checking Graphite Studio...
cargo check
if errorlevel 1 exit /b 1

echo Building optimized Windows binary...
cargo build --release
if errorlevel 1 exit /b 1

echo Done: target\release\graphite-studio.exe
endlocal
