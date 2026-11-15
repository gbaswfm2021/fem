@echo off
setlocal
cd /d "%~dp0"
set "CARGO_TARGET_DIR=%~dp0..\.build"
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
"%RUSTUP%" run stable-x86_64-pc-windows-gnu cargo build --release --offline --locked --target x86_64-pc-windows-gnu --manifest-path Cargo.toml --bin fullmeta-fem
if errorlevel 1 exit /b %errorlevel%
"..\.build\x86_64-pc-windows-gnu\release\fullmeta-fem.exe" --mesh-only %*
exit /b %errorlevel%
