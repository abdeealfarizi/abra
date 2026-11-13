# Builds dist\ABRA-Setup-<version>.exe with Inno Setup 6.
# Usage:  powershell -File installer\build-installer.ps1 [-Source <folder with abra.exe + abra-guard.exe>] [-Version 0.3.0]
param(
  [string]$Source = (Join-Path $PSScriptRoot "..\target\release"),
  [string]$Version = "0.3.0"
)
$ErrorActionPreference = "Stop"

$iscc = (Get-Command iscc.exe -ErrorAction SilentlyContinue).Source
if (-not $iscc) {
  foreach ($p in @("$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe", "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe", "$env:ProgramFiles\Inno Setup 6\ISCC.exe")) {
    if (Test-Path $p) { $iscc = $p; break }
  }
}
if (-not $iscc) { throw "Inno Setup 6 not found. Install it from https://jrsoftware.org/isinfo.php (or: winget install JRSoftware.InnoSetup)." }

$Source = (Resolve-Path $Source).Path
foreach ($f in "abra.exe", "abra-guard.exe") {
  if (-not (Test-Path (Join-Path $Source $f))) { throw "Missing $f in $Source. Build first: cargo build --release" }
}

New-Item -ItemType Directory -Force (Join-Path $PSScriptRoot "..\dist") | Out-Null
& $iscc "/DAppVersion=$Version" "/DSourceDir=$Source" (Join-Path $PSScriptRoot "ABRA.iss")
if ($LASTEXITCODE -ne 0) { throw "ISCC failed ($LASTEXITCODE)" }
Write-Host "Done: dist\ABRA-Setup-$Version.exe"
