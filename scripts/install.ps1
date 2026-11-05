param(
    [string]$Source,
    [switch]$NoAutostart,
    [switch]$NoStart
)

$ErrorActionPreference = "Stop"

$AppName = "Brightfly"
$ExeName = "brightfly.exe"
$InstallDir = Join-Path $env:LOCALAPPDATA "Programs\Brightfly"
$RunKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run"
$UninstallKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\$AppName"
$ShortcutPath = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\$AppName.lnk"

function Find-Source {
    param([string]$Hint)

    $candidates = @()
    if ($Hint) { $candidates += $Hint }
    $candidates += $PSScriptRoot
    $candidates += (Join-Path $PSScriptRoot "..\target\release")

    foreach ($candidate in $candidates) {
        if (-not $candidate) { continue }
        $directory = [System.IO.Path]::GetFullPath($candidate)
        $exe = Join-Path $directory $ExeName
        if (Test-Path -LiteralPath $exe) {
            return [System.IO.Path]::GetDirectoryName($exe)
        }
    }

    throw "brightfly.exe not found. Run 'cargo build --release' first, or pass -Source <folder>."
}

function Stop-Running {
    $processes = Get-Process -Name "brightfly" -ErrorAction SilentlyContinue
    if (-not $processes) { return }

    $processes | Stop-Process -Force -ErrorAction SilentlyContinue
    for ($i = 0; $i -lt 20; $i++) {
        Start-Sleep -Milliseconds 100
        if (-not (Get-Process -Name "brightfly" -ErrorAction SilentlyContinue)) { return }
    }
}

function Read-AppVersion {
    param([string]$Path)

    $info = (Get-Item -LiteralPath $Path).VersionInfo
    if ($info.FileVersion) { return $info.FileVersion.Trim() }
    return "0.0.0"
}

function Find-Payload {
    param([string]$File, [string]$Primary)

    $candidates = @(
        (Join-Path $Primary $File),
        (Join-Path $PSScriptRoot $File),
        (Join-Path $PSScriptRoot "..\$File")
    )
    foreach ($candidate in $candidates) {
        if ($candidate -and (Test-Path -LiteralPath $candidate)) {
            return [System.IO.Path]::GetFullPath($candidate)
        }
    }
    return $null
}

$sourceDir = Find-Source -Hint $Source
$sourceExe = Join-Path $sourceDir $ExeName
$targetExe = Join-Path $InstallDir $ExeName
$version = Read-AppVersion -Path $sourceExe

Write-Host "Brightfly $version"
Write-Host "  from : $sourceDir"
Write-Host "  to   : $InstallDir"

Stop-Running

New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null

$payload = @($ExeName, "README.md", "uninstall.ps1", "uninstall.cmd")
foreach ($file in $payload) {
    $from = Find-Payload -File $file -Primary $sourceDir
    if (-not $from) {
        if ($file -eq "uninstall.ps1") {
            throw "uninstall.ps1 not found; the package is incomplete."
        }
        Write-Host "Skipping $file (not found)"
        continue
    }
    $to = Join-Path $InstallDir $file
    if ([System.IO.Path]::GetFullPath($from) -eq [System.IO.Path]::GetFullPath($to)) { continue }
    Copy-Item -LiteralPath $from -Destination $to -Force
}

$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($ShortcutPath)
$shortcut.TargetPath = $targetExe
$shortcut.WorkingDirectory = $InstallDir
$shortcut.IconLocation = "$targetExe,0"
$shortcut.Description = "Brightness slider for external monitors"
$shortcut.Save()

New-Item -Path $UninstallKey -Force | Out-Null
New-ItemProperty -Path $UninstallKey -Name "DisplayName" -Value $AppName -PropertyType String -Force | Out-Null
New-ItemProperty -Path $UninstallKey -Name "DisplayVersion" -Value $version -PropertyType String -Force | Out-Null
New-ItemProperty -Path $UninstallKey -Name "DisplayIcon" -Value "$targetExe,0" -PropertyType String -Force | Out-Null
New-ItemProperty -Path $UninstallKey -Name "InstallLocation" -Value $InstallDir -PropertyType String -Force | Out-Null
$uninstallScript = Join-Path $InstallDir "uninstall.ps1"
$uninstallCommand = "powershell.exe -NoProfile -ExecutionPolicy Bypass -File `"$uninstallScript`""
New-ItemProperty -Path $UninstallKey -Name "UninstallString" -Value $uninstallCommand -PropertyType String -Force | Out-Null
New-ItemProperty -Path $UninstallKey -Name "QuietUninstallString" -Value "$uninstallCommand -Quiet" -PropertyType String -Force | Out-Null
New-ItemProperty -Path $UninstallKey -Name "NoModify" -Value 1 -PropertyType DWord -Force | Out-Null
New-ItemProperty -Path $UninstallKey -Name "NoRepair" -Value 1 -PropertyType DWord -Force | Out-Null
$sizeKb = [int]((Get-Item -LiteralPath $targetExe).Length / 1KB)
New-ItemProperty -Path $UninstallKey -Name "EstimatedSize" -Value $sizeKb -PropertyType DWord -Force | Out-Null

if ($NoAutostart) {
    Remove-ItemProperty -Path $RunKey -Name $AppName -ErrorAction SilentlyContinue
    Write-Host "Autostart: off"
} else {
    New-ItemProperty -Path $RunKey -Name $AppName -Value "`"$targetExe`"" -PropertyType String -Force | Out-Null
    Write-Host "Autostart: on"
}

if (-not $NoStart) {
    Start-Process -FilePath $targetExe -WorkingDirectory $InstallDir | Out-Null
    Write-Host "Brightfly started."
}

Write-Host ""
Write-Host "Done. Uninstall from Settings > Apps, or run:"
Write-Host "  $uninstallScript"