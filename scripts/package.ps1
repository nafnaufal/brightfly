param(
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$manifest = Get-Content (Join-Path $root "Cargo.toml") -Raw
if ($manifest -notmatch '(?m)^version\s*=\s*"([^"]+)"') {
    throw "Version not found in Cargo.toml"
}
$version = $Matches[1]

if (-not $SkipBuild) {
    cargo build --release --bin brightfly
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build --release --bin brightfly failed"
    }
}

$exe = Join-Path $root "target\release\brightfly.exe"
if (-not (Test-Path $exe)) {
    throw "Exe not found: $exe"
}

$iscc = $env:ISCC
if (-not $iscc) {
    $iscc = Get-Command iscc -ErrorAction SilentlyContinue |
        Select-Object -First 1 -ExpandProperty Source
}
if (-not $iscc) {
    foreach ($candidate in @(
        (Join-Path $env:LOCALAPPDATA "Programs\Inno Setup 6\ISCC.exe"),
        "C:\Program Files (x86)\Inno Setup 6\ISCC.exe",
        "C:\Program Files\Inno Setup 6\ISCC.exe"
    )) {
        if (Test-Path $candidate) { $iscc = $candidate; break }
    }
}
if (-not $iscc) {
    throw "ISCC.exe not found. Install Inno Setup 6: winget install JRSoftware.InnoSetup"
}

$dist = Join-Path $root "dist"
New-Item -ItemType Directory -Path $dist -Force | Out-Null

& $iscc "/DAppVersion=$version" (Join-Path $root "installer\brightfly.iss")
if ($LASTEXITCODE -ne 0) {
    throw "Inno Setup failed"
}

$setup = Join-Path $dist "brightfly-$version-setup.exe"
if (-not (Test-Path $setup)) {
    throw "Installer not found: $setup"
}

$portable = Join-Path $dist "brightfly-$version.exe"
Copy-Item $exe $portable -Force

$stage = Join-Path $dist "brightfly-$version-win64"
if (Test-Path $stage) {
    Remove-Item $stage -Recurse -Force
}
New-Item -ItemType Directory -Path $stage -Force | Out-Null
Copy-Item $exe (Join-Path $stage "brightfly.exe")
Copy-Item (Join-Path $root "README.md") $stage
Copy-Item (Join-Path $root "LICENSE") $stage

$zip = Join-Path $dist "brightfly-$version-win64.zip"
if (Test-Path $zip) {
    Remove-Item $zip -Force
}
Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zip -CompressionLevel Optimal

Get-ChildItem $dist -File |
    Where-Object { $_.Name -like "brightfly-$version-*" } |
    Select-Object Name, Length