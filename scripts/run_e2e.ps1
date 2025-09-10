param(
  [string]$Package = "atomd"
)
$ErrorActionPreference = 'SilentlyContinue'
Write-Host "🔧 Stopping running processes (atom-ide/atomd)..."
taskkill /F /IM atom-ide.exe | Out-Null
taskkill /F /IM atomd.exe     | Out-Null
Start-Sleep -Milliseconds 300
Write-Host "✅ Processes stopped (if any). Running tests..."
$env:RUST_LOG='info'
& cargo test -p $Package -- --test-threads=1
