param(
    [switch]$Quiet,
    [switch]$PurgeConfig
)

$ErrorActionPreference = "Stop"

$AppName = "Brightfly"
$InstallDir = Join-Path $env:LOCALAPPDATA "Programs\Brightfly"
$ConfigDir = Join-Path $env:APPDATA "Brightfly"
$RunKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run"
$UninstallKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\$AppName"
$ShortcutPath = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\$AppName.lnk"

function Write-Step {
    param([string]$Message)
    if (-not $Quiet) { Write-Host $Message }
}

$processes = Get-Process -Name "brightfly" -ErrorAction SilentlyContinue
if ($processes) {
    $processes | Stop-Process -Force -ErrorAction SilentlyContinue
    for ($i = 0; $i -lt 20; $i++) {
        Start-Sleep -Milliseconds 100
        if (-not (Get-Process -Name "brightfly" -ErrorAction SilentlyContinue)) { break }
    }
    Write-Step "Proses dihentikan."
}

if (Get-ItemProperty -Path $RunKey -Name $AppName -ErrorAction SilentlyContinue) {
    Remove-ItemProperty -Path $RunKey -Name $AppName -Force
    Write-Step "Entri autostart dihapus."
}

if (Test-Path -LiteralPath $UninstallKey) {
    Remove-Item -LiteralPath $UninstallKey -Recurse -Force
    Write-Step "Entri Apps & features dihapus."
}

if (Test-Path -LiteralPath $ShortcutPath) {
    Remove-Item -LiteralPath $ShortcutPath -Force
    Write-Step "Shortcut Start Menu dihapus."
}

if ($PurgeConfig) {
    if (Test-Path -LiteralPath $ConfigDir) {
        Remove-Item -LiteralPath $ConfigDir -Recurse -Force
        Write-Step "Config dihapus: $ConfigDir"
    }
} else {
    Write-Step "Config dipertahankan: $ConfigDir"
}

$runningFromInstallDir = $PSScriptRoot -and
    ([System.IO.Path]::GetFullPath($PSScriptRoot) -eq [System.IO.Path]::GetFullPath($InstallDir))

if (Test-Path -LiteralPath $InstallDir) {
    if ($runningFromInstallDir) {
        Start-Process -FilePath "cmd.exe" `
            -ArgumentList "/c", "ping -n 3 127.0.0.1 >nul & rmdir /s /q `"$InstallDir`"" `
            -WindowStyle Hidden | Out-Null
        Write-Step "Folder instalasi akan dihapus sebentar lagi: $InstallDir"
    } else {
        $removed = $true
        for ($i = 0; $i -lt 10; $i++) {
            try {
                Remove-Item -LiteralPath $InstallDir -Recurse -Force -ErrorAction Stop
                $removed = $true
                break
            } catch {
                $removed = $false
                Start-Sleep -Milliseconds 200
            }
        }

        if ($removed) {
            Write-Step "Folder instalasi dihapus: $InstallDir"
        } else {
            Start-Process -FilePath "cmd.exe" `
                -ArgumentList "/c", "ping -n 3 127.0.0.1 >nul & rmdir /s /q `"$InstallDir`"" `
                -WindowStyle Hidden | Out-Null
            Write-Step "Folder instalasi dihapus setelah proses selesai."
        }
    }
}

Write-Step "Brightfly sudah di-uninstall."