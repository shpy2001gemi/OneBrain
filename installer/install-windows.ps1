# OneBrain Installer — Windows
# Installs to %LOCALAPPDATA%\OneBrain

$ErrorActionPreference = 'Stop'

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectRoot = Split-Path -Parent $ScriptDir
$BuildDir = Join-Path $ProjectRoot 'build'

Write-Host ''
Write-Host '  +================================================+' -ForegroundColor Cyan
Write-Host '  |     OneBrain Installer — Windows               |' -ForegroundColor Cyan
Write-Host '  +================================================+' -ForegroundColor Cyan
Write-Host ''

# Check build
if (-not (Test-Path "$BuildDir\bin\onebrain.exe")) {
    Write-Host '  X Build not found. Run .\build.ps1 first.' -ForegroundColor Red
    exit 1
}

$InstallDir = Join-Path $env:LOCALAPPDATA 'OneBrain'
$InstallBin = Join-Path $InstallDir 'bin'
$InstallWeb = Join-Path $InstallDir 'web'

Write-Host "Installing to: $InstallDir" -ForegroundColor White
Write-Host ''

New-Item -Path $InstallBin -ItemType Directory -Force | Out-Null
New-Item -Path $InstallWeb -ItemType Directory -Force | Out-Null

Write-Host '[1/4] Installing binary...' -ForegroundColor Yellow
Copy-Item "$BuildDir\bin\onebrain.exe" "$InstallBin\onebrain.exe" -Force
Write-Host "  OK $InstallBin\onebrain.exe" -ForegroundColor Green

Write-Host '[2/4] Installing web dashboard...' -ForegroundColor Yellow
Copy-Item "$BuildDir\web\*" $InstallWeb -Recurse -Force
Write-Host "  OK $InstallWeb" -ForegroundColor Green

Write-Host '[3/4] Creating launcher...' -ForegroundColor Yellow
$LauncherContent = @"
@echo off
title OneBrain Dashboard
set BIN=$InstallBin\onebrain.exe
set WEB=$InstallWeb
echo Starting OneBrain with Web Dashboard...
echo.
"%BIN%" start --api --web-dir "%WEB%" %*
pause
"@
Set-Content -Path "$InstallDir\OneBrain Dashboard.bat" -Value $LauncherContent -Encoding UTF8
Write-Host "  OK $InstallDir\OneBrain Dashboard.bat" -ForegroundColor Green

Write-Host '[4/4] Adding to PATH...' -ForegroundColor Yellow
$CurrentPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if ($CurrentPath -notlike "*$InstallBin*") {
    [Environment]::SetEnvironmentVariable('Path', "$InstallBin;$CurrentPath", 'User')
    Write-Host "  OK Added $InstallBin to user PATH" -ForegroundColor Green
    Write-Host '  Note: Restart terminal for PATH changes to take effect' -ForegroundColor Yellow
} else {
    Write-Host '  OK Already in PATH' -ForegroundColor Green
}

Write-Host ''
Write-Host '================================================' -ForegroundColor Cyan
Write-Host 'INSTALLATION COMPLETE!' -ForegroundColor Green
Write-Host ''
Write-Host 'Commands:' -ForegroundColor White
Write-Host '  onebrain start                # CLI only'
Write-Host '  onebrain start --api          # CLI + API'
Write-Host "  & '$InstallDir\OneBrain Dashboard.bat'  # Full dashboard"
Write-Host ''
Write-Host 'Or run directly:' -ForegroundColor White
Write-Host "  onebrain start --api --web-dir `"$InstallWeb`"" -ForegroundColor Yellow
Write-Host ''
Write-Host 'Then open: http://localhost:4280' -ForegroundColor Yellow
Write-Host 'Token: onebrain-dev-token' -ForegroundColor Yellow
Write-Host '================================================' -ForegroundColor Cyan
