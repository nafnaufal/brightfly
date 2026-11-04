# Membuat paket portable: dist/brightfly-<versi>-win64.zip
#
# Isi paket: brightfly.exe dan README.md. Tidak ada installer; cukup
# ekstrak lalu jalankan exe.

param(
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

if (-not $SkipBuild) {
    cargo build --release
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build --release gagal"
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

if (Test-Path $zip) {
    Remove-Item $zip -Force
}
Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zip -CompressionLevel Optimal

$size = (Get-Item $zip).Length
Write-Host ("Paket: {0} ({1:N0} byte)" -f $zip, $size)