$ErrorActionPreference = 'Stop'
Write-Host ''
Write-Host '  +================================================+' -ForegroundColor Cyan
Write-Host '  |   OneBrain â€” Installer                         |' -ForegroundColor Cyan
Write-Host '  +================================================+' -ForegroundColor Cyan
Write-Host ''

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
if (-not (Test-Path "$ScriptDir\bin\onebrain.exe")) {
    Write-Host 'X Package incomplete' -ForegroundColor Red; exit 1
}

$InstallDir = Join-Path $env:LOCALAPPDATA 'OneBrain'
$InstallBin = Join-Path $InstallDir 'bin'
$InstallWeb = Join-Path $InstallDir 'web'
Write-Host "Install to: $InstallDir"
Write-Host ''

New-Item -Path $InstallDir -ItemType Directory -Force | Out-Null
$Manifest = Join-Path $InstallDir '.installed-by-onebrain'
'' | Set-Content $Manifest -Encoding UTF8

Write-Host '[1/4] Installing OneBrain...' -ForegroundColor Yellow
New-Item -Path $InstallBin -ItemType Directory -Force | Out-Null
New-Item -Path $InstallWeb -ItemType Directory -Force | Out-Null
Copy-Item "$ScriptDir\bin\onebrain.exe" "$InstallBin\onebrain.exe" -Force
Write-Host "  OK Binary: $InstallBin\onebrain.exe" -ForegroundColor Green
Copy-Item "$ScriptDir\web\*" $InstallWeb -Recurse -Force
Write-Host "  OK Web:    $InstallWeb" -ForegroundColor Green

$Bat = @"
@echo off
title OneBrain Dashboard
where ollama >nul 2>nul && (curl -s http://localhost:11434/api/tags >nul 2>nul || (echo Starting Ollama... & start /B ollama serve >nul 2>nul & timeout /t 3 >nul))
echo Starting OneBrain...
"$InstallBin\onebrain.exe" start --api --web-dir "$InstallWeb" %*
pause
"@
Set-Content -Path "$InstallDir\OneBrain Dashboard.bat" -Value $Bat -Encoding UTF8
Write-Host "  OK Launcher created" -ForegroundColor Green
Write-Host ''

Write-Host '[2/4] Configuring PATH...' -ForegroundColor Yellow
$P = [Environment]::GetEnvironmentVariable('Path','User')
if ($P -notlike "*$InstallBin*") {
    [Environment]::SetEnvironmentVariable('Path',"$InstallBin;$P",'User')
    $env:Path = "$InstallBin;$env:Path"
    Write-Host '  OK Added to PATH' -ForegroundColor Green
} else { Write-Host '  OK Already in PATH' -ForegroundColor Green }
Write-Host ''

Write-Host '[3/4] AI Engine (Ollama)...' -ForegroundColor Yellow
if (Get-Command ollama -ErrorAction SilentlyContinue) {
    Write-Host '  OK Ollama: already installed' -ForegroundColor Green
} else {
    $R = Read-Host '  Ollama not found. Install for AI features? [Y/n]'
    if (-not $R -or $R -match '^[Yy]') {
        $W = $false; try { winget --version | Out-Null; $W = $true } catch {}
        if ($W) { winget install --id Ollama.Ollama -e --accept-source-agreements --accept-package-agreements --silent }
        else {
            $F = Join-Path $env:TEMP 'OllamaSetup.exe'
            Invoke-WebRequest -Uri 'https://ollama.ai/download/OllamaSetup.exe' -OutFile $F -UseBasicParsing
            Start-Process $F -ArgumentList '/VERYSILENT' -Wait
            Remove-Item $F -ErrorAction SilentlyContinue
        }
        $env:Path = [Environment]::GetEnvironmentVariable('Path','Machine')+';'+[Environment]::GetEnvironmentVariable('Path','User')
        if (Get-Command ollama -ErrorAction SilentlyContinue) {
            Add-Content $Manifest 'ollama'
            Write-Host '  OK Ollama installed' -ForegroundColor Green
        } else { Write-Host '  ! Install failed' -ForegroundColor Yellow }
    } else { Write-Host '  - Skipped' -ForegroundColor Yellow }
}
Write-Host ''

Write-Host '[4/4] AI Model...' -ForegroundColor Yellow
if (Get-Command ollama -ErrorAction SilentlyContinue) {
    $R = Read-Host '  Download AI model qwen3:8b (~4.9GB)? [Y/n]'
    if (-not $R -or $R -match '^[Yy]') {
        try {
            $P = Start-Process ollama -ArgumentList 'serve' -PassThru -WindowStyle Hidden -ErrorAction SilentlyContinue
            Start-Sleep 3; ollama pull qwen3:8b
            Write-Host '  OK Model ready' -ForegroundColor Green
            if ($P -and -not $P.HasExited) { $P | Stop-Process -Force -ErrorAction SilentlyContinue }
        } catch { Write-Host "  ! Failed - run 'ollama pull qwen3:8b' later" -ForegroundColor Yellow }
    }
} else { Write-Host '  - Ollama not installed' }

Write-Host ''
Write-Host '================================================' -ForegroundColor Cyan
Write-Host '  OneBrain installed!' -ForegroundColor Green
Write-Host '  Run: onebrain start --api' -ForegroundColor Yellow
Write-Host '  Open: http://localhost:4280' -ForegroundColor Yellow
Write-Host '================================================' -ForegroundColor Cyan
Read-Host 'Press Enter to close'
