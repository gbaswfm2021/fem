@echo off
setlocal
cd /d "%~dp0"
set "CARGO_TARGET_DIR=%~dp0..\.runtime"
set "MODEL_JSON=%~1"
if not defined MODEL_JSON (
  echo Usage: run-mesher.cmd path\to\fem_model.json [--component componentId]
  exit /b 2
)
where rustup >nul 2>nul
if errorlevel 1 (
  set "RUSTUP=%USERPROFILE%\.cargo\bin\rustup.exe"
) else (
  set "RUSTUP=rustup"
)
"%RUSTUP%" run stable-x86_64-pc-windows-gnu cargo build --release --offline --target x86_64-pc-windows-gnu
if errorlevel 1 exit /b %errorlevel%
"..\.runtime\x86_64-pc-windows-gnu\release\fullmeta-mesh.exe" build %*
exit /b %errorlevel%
