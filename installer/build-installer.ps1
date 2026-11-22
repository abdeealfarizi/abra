param(
  [string]$Version = "0.6.1",
  [switch]$NoBuild
)
$ErrorActionPreference = "Stop"

$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$Source = Join-Path $root "target\release"
$dist = Join-Path $root "dist"

$iscc = (Get-Command iscc.exe -ErrorAction SilentlyContinue).Source
if (-not $iscc) {
  foreach ($p in @("$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe", "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe", "$env:ProgramFiles\Inno Setup 6\ISCC.exe")) {
    if (Test-Path $p) { $iscc = $p; break }
  }
}
if (-not $iscc) { throw "Inno Setup 6 not found. Install it from https://jrsoftware.org/isinfo.php (or: winget install JRSoftware.InnoSetup)." }

if (-not $NoBuild) {
  $sep = [char]0x1f
  $env:CARGO_ENCODED_RUSTFLAGS = "--remap-path-prefix=$env:USERPROFILE=~$sep--remap-path-prefix=$root=abra"
  Push-Location $root
  try {
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed ($LASTEXITCODE)" }
  } finally {
    Pop-Location
    Remove-Item Env:\CARGO_ENCODED_RUSTFLAGS -ErrorAction SilentlyContinue
  }
}

$files = @("abra.exe", "abra-guard.exe")
foreach ($f in $files) {
  if (-not (Test-Path (Join-Path $Source $f))) { throw "Missing $f in $Source." }
}

$needles = @($env:USERNAME, $env:USERPROFILE, $root) | Where-Object { $_ -and $_.Length -ge 3 } | Select-Object -Unique
foreach ($f in $files) {
  $bytes = [IO.File]::ReadAllBytes((Join-Path $Source $f))
  $ascii = [Text.Encoding]::ASCII.GetString($bytes)
  $wide = [Text.Encoding]::Unicode.GetString($bytes)
  foreach ($n in $needles) {
    if ($ascii.IndexOf($n, [StringComparison]::OrdinalIgnoreCase) -ge 0 -or $wide.IndexOf($n, [StringComparison]::OrdinalIgnoreCase) -ge 0) {
      throw "Privacy check failed: $f still contains a local path or user name. Rebuild without -NoBuild."
    }
  }
}
Write-Host "Privacy check passed: no local paths or user name inside the binaries."

New-Item -ItemType Directory -Force $dist | Out-Null
& $iscc "/DAppVersion=$Version" "/DSourceDir=$Source" (Join-Path $PSScriptRoot "ABRA.iss")
if ($LASTEXITCODE -ne 0) { throw "ISCC failed ($LASTEXITCODE)" }

$stage = Join-Path $dist "ABRA-$Version"
Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force (Join-Path $stage "fonts") | Out-Null
foreach ($f in $files) { Copy-Item (Join-Path $Source $f) $stage }
Copy-Item (Join-Path $root "LICENSE.md") $stage
Copy-Item (Join-Path $root "assets\fonts\*") (Join-Path $stage "fonts")
$zip = Join-Path $dist "ABRA-$Version-win-x64.zip"
Remove-Item -Force $zip -ErrorAction SilentlyContinue
Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zip
Remove-Item -Recurse -Force $stage

Write-Host "Done: dist\ABRA-Setup-$Version.exe and dist\ABRA-$Version-win-x64.zip"
