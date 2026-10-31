# Builds the OpenTS2 Windows installer: target\installer\OpenTS2-<version>-setup.exe
#
#   powershell -ExecutionPolicy Bypass -File tools\installer\build.ps1
#
# Needs Rust (MSVC) and Inno Setup 6 (winget install JRSoftware.InnoSetup).
$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..\..')
Set-Location $root

# The launcher's version is the product version.
$version = (Select-String -Path 'crates\launcher\Cargo.toml' -Pattern '^version\s*=\s*"([^"]+)"').Matches[0].Groups[1].Value
Write-Host "OpenTS2 $version"

cargo build --release --bin opents2 --bin ts2-viewer
if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }

$iscc = @(
    (Get-Command ISCC.exe -ErrorAction SilentlyContinue).Source,
    "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe",
    "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
    "$env:ProgramFiles\Inno Setup 6\ISCC.exe"
) | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
if (-not $iscc) { throw 'Inno Setup 6 not found: winget install JRSoftware.InnoSetup' }

& $iscc /Qp "/DAppVersion=$version" 'tools\installer\opents2.iss'
if ($LASTEXITCODE -ne 0) { throw 'Inno Setup failed' }
Get-Item "target\installer\OpenTS2-$version-setup.exe" | Select-Object FullName, Length
