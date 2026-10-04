# Starts the Catan release server detached so it never blocks the agent session.
# Build first (`cargo build --release`), then: .\scripts\serve-bg.ps1
param(
  [string]$Addr = "127.0.0.1:8090",
  [string]$DataDir = "$env:TEMP\opencode\catan-data"
)

$root = Split-Path -Parent $PSScriptRoot
$exe = Join-Path $root "target\release\catan.exe"
$tmp = Join-Path $env:TEMP "opencode"
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
New-Item -ItemType Directory -Force -Path $DataDir | Out-Null

$pidFile = Join-Path $tmp "catan.pid"
$log = Join-Path $tmp "catan-server.log"
$errlog = Join-Path $tmp "catan-server.err.log"

if (Test-Path $pidFile) { Stop-Process -Id (Get-Content $pidFile) -Force -ErrorAction SilentlyContinue }
Get-Process catan -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 500

Remove-Item $log, $errlog -ErrorAction SilentlyContinue
$env:CATAN_ADDR = $Addr
$env:CATAN_DATA_DIR = $DataDir
$p = Start-Process -FilePath $exe -WorkingDirectory $root `
  -RedirectStandardOutput $log -RedirectStandardError $errlog `
  -PassThru -WindowStyle Hidden
$p.Id | Out-File $pidFile
"started catan pid=$($p.Id) addr=$Addr data=$DataDir"
