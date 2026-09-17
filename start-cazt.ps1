$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

$env:CAZT_BIND = "0.0.0.0"
$env:CAZT_PORT = "8787"
$env:CAZT_PUBLIC_HOST = "192.168.23.207"
$env:CAZT_WEB_ROOT = Join-Path $PSScriptRoot "frontend\dist"
$env:CAZT_LOG = "info"
$env:CARGO_TARGET_DIR = Join-Path $PSScriptRoot "server\target"

if (-not (Test-Path $env:CAZT_WEB_ROOT)) {
    Push-Location frontend
    npm install
    npm run build
    Pop-Location
}

Write-Host "Starting Cazt at http://192.168.23.207:8787"
Set-Location server
cargo run --release
