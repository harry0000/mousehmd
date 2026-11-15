# Builds the driver DLL and copies it to where SteamVR expects it.
# Run with SteamVR closed: the DLL is locked while vrserver.exe is running.
$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

if (Get-Process vrserver -ErrorAction SilentlyContinue) {
    Write-Host 'SteamVR is running. Close it first, then run this script again.' -ForegroundColor Red
    exit 1
}

cargo build --release
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$dst = Join-Path $PSScriptRoot 'driver\mousehmd\bin\win64'
New-Item -ItemType Directory -Force $dst | Out-Null
Copy-Item (Join-Path $PSScriptRoot 'target\release\driver_mousehmd.dll') $dst -Force
Write-Host "OK: $dst\driver_mousehmd.dll" -ForegroundColor Green
