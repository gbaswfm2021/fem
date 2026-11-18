$ErrorActionPreference = 'Stop'

# Build the browser and native runtimes from the same sources. The native
# builder keeps a backup before replacing the FEM-local four-file bundle.
$browserBuilder = Join-Path $PSScriptRoot 'js/runtime/build-wasm.mjs'
$nativeBuilder = Join-Path $PSScriptRoot 'fem-rust/prebuilt-tools/build-local-bundle.mjs'
& node $browserBuilder
if ($LASTEXITCODE -ne 0) { throw "FEM browser runtime build failed ($LASTEXITCODE)." }
& node $nativeBuilder --offline --stage
if ($LASTEXITCODE -ne 0) { throw "FEM native runtime build failed ($LASTEXITCODE)." }
