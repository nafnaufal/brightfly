param(
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

if (-not $SkipBuild) {
    cargo build --release --bin brightfly
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build --release --bin brightfly gagal"
    }
}

$manifest = Get-Content (Join-Path $root "Cargo.toml") -Raw
if ($manifest -notmatch '(?m)^version\s*=\s*"([^"]+)"') {
    throw "Versi tidak ditemukan di Cargo.toml"
}
$version = $Matches[1]

$exe = Join-Path $root "target\release\brightfly.exe"
if (-not (Test-Path $exe)) {
    throw "Exe tidak ditemukan: $exe"
}

$name = "brightfly-$version-win64"
$dist = Join-Path $root "dist"
$stage = Join-Path $dist $name
$zip = Join-Path $dist "$name.zip"

if (Test-Path $stage) {
    Remove-Item $stage -Recurse -Force
}
New-Item -ItemType Directory -Path $stage -Force | Out-Null

Copy-Item $exe $stage
Copy-Item (Join-Path $root "README.md") $stage

$env:BRIGHTFLY_PAYLOAD_DIR = $stage
try {
    cargo build --release --bin brightfly-setup
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build --release --bin brightfly-setup gagal"
    }
} finally {
    Remove-Item Env:\BRIGHTFLY_PAYLOAD_DIR -ErrorAction SilentlyContinue
}

$setup = Join-Path $root "target\release\brightfly-setup.exe"
if (-not (Test-Path $setup)) {
    throw "Installer tidak ditemukan: $setup"
}
Copy-Item $setup (Join-Path $stage "setup.exe")

if (Test-Path $zip) {
    Remove-Item $zip -Force
}
Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zip -CompressionLevel Optimal

$size = (Get-Item $zip).Length
Write-Host ("Paket: {0} ({1:N0} byte)" -f $zip, $size)