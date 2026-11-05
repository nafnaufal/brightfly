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
    Write-Step "Process stopped."
}

if (Get-ItemProperty -Path $RunKey -Name $AppName -ErrorAction SilentlyContinue) {
    Remove-ItemProperty -Path $RunKey -Name $AppName -Force
    Write-Step "Autostart entry removed."
}

if (Test-Path -LiteralPath $UninstallKey) {
    Remove-Item -LiteralPath $UninstallKey -Recurse -Force
    Write-Step "Apps & features entry removed."
}

if (Test-Path -LiteralPath $ShortcutPath) {
    Remove-Item -LiteralPath $ShortcutPath -Force
    Write-Step "Start Menu shortcut removed."
}

if ($PurgeConfig) {
    if (Test-Path -LiteralPath $ConfigDir) {
        Remove-Item -LiteralPath $ConfigDir -Recurse -Force
        Write-Step "Config removed: $ConfigDir"
    }
} else {
    Write-Step "Config kept: $ConfigDir"
}

$runningFromInstallDir = $PSScriptRoot -and
    ([System.IO.Path]::GetFullPath($PSScriptRoot) -eq [System.IO.Path]::GetFullPath($InstallDir))

if (Test-Path -LiteralPath $InstallDir) {
    if ($runningFromInstallDir) {
        Start-Process -FilePath "cmd.exe" `
            -ArgumentList "/c", "ping -n 3 127.0.0.1 >nul & rmdir /s /q `"$InstallDir`"" `
            -WindowStyle Hidden | Out-Null
        Write-Step "Install folder will be removed shortly: $InstallDir"
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
            Write-Step "Install folder removed: $InstallDir"
        } else {
            Start-Process -FilePath "cmd.exe" `
                -ArgumentList "/c", "ping -n 3 127.0.0.1 >nul & rmdir /s /q `"$InstallDir`"" `
                -WindowStyle Hidden | Out-Null
            Write-Step "Install folder will be removed after the process exits."
        }
    }
}

Write-Step "Brightfly has been uninstalled."