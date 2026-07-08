# ══════════════════════════════════════════════════════════════
# OneBrain Release Builder — Windows
#
# Developer runs this to create distributable package.
# Output: onebrain-windows-x64.zip
#
# Users only need to extract and run install.ps1
# ══════════════════════════════════════════════════════════════

$ErrorActionPreference = 'Stop'

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectRoot = Split-Path -Parent $ScriptDir
$SrcDir = Join-Path $ProjectRoot 'src'
$ReleaseDir = Join-Path $ProjectRoot 'release'

$PackageName = 'onebrain-windows-x64'

Write-Host ''
Write-Host '  +================================================+' -ForegroundColor Cyan
Write-Host '  |   OneBrain — Release Builder (Windows)         |' -ForegroundColor Cyan
Write-Host "  |   Target: $PackageName                |" -ForegroundColor Cyan
Write-Host '  +================================================+' -ForegroundColor Cyan
Write-Host ''

# ── Check prerequisites ──────────────────────────────────────
Write-Host '[1/5] Checking build tools...' -ForegroundColor Yellow
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Host '  X Rust required to build' -ForegroundColor Red; exit 1
}
if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
    Write-Host '  X Node.js required to build' -ForegroundColor Red; exit 1
}
Write-Host "  OK Rust: $(rustc --version)" -ForegroundColor Green
Write-Host "  OK Node: $(node --version)" -ForegroundColor Green
Write-Host ''

# ── Build CLI ────────────────────────────────────────────────
Write-Host '[2/5] Building CLI (release mode)...' -ForegroundColor Yellow
Push-Location $SrcDir
cargo build --release -p onebrain-cli
if ($LASTEXITCODE -ne 0) { Write-Host '  X Build failed' -ForegroundColor Red; exit 1 }
Pop-Location
Write-Host '  OK CLI built' -ForegroundColor Green
Write-Host ''

# ── Build Web ────────────────────────────────────────────────
Write-Host '[3/5] Building Web Dashboard...' -ForegroundColor Yellow
Push-Location (Join-Path $SrcDir 'onebrain-web')
if (-not (Test-Path 'node_modules')) { npm install --silent }
npm run build
if ($LASTEXITCODE -ne 0) { Write-Host '  X Web build failed' -ForegroundColor Red; exit 1 }
Pop-Location
Write-Host '  OK Web built' -ForegroundColor Green
Write-Host ''

# ── Create package ───────────────────────────────────────────
Write-Host '[4/5] Creating release package...' -ForegroundColor Yellow

$DistDir = Join-Path $ReleaseDir $PackageName
if (Test-Path $DistDir) { Remove-Item $DistDir -Recurse -Force }
New-Item -Path "$DistDir\bin" -ItemType Directory -Force | Out-Null
New-Item -Path "$DistDir\web" -ItemType Directory -Force | Out-Null

# Copy binary
Copy-Item "$SrcDir\target\release\onebrain.exe" "$DistDir\bin\onebrain.exe"

# Copy web
Copy-Item "$SrcDir\onebrain-web\dist\*" "$DistDir\web\" -Recurse

# ── Create install.ps1 ───────────────────────────────────────
$InstallScript = @'
# OneBrain Installer
$ErrorActionPreference = 'Stop'

Write-Host ''
Write-Host '  +================================================+' -ForegroundColor Cyan
Write-Host '  |   OneBrain — Installer                         |' -ForegroundColor Cyan
Write-Host '  +================================================+' -ForegroundColor Cyan
Write-Host ''

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path

# Validate package
if (-not (Test-Path "$ScriptDir\bin\onebrain.exe")) {
    Write-Host 'X Package incomplete: bin\onebrain.exe not found' -ForegroundColor Red
    exit 1
}

$InstallDir = Join-Path $env:LOCALAPPDATA 'OneBrain'
$InstallBin = Join-Path $InstallDir 'bin'
$InstallWeb = Join-Path $InstallDir 'web'

Write-Host "Install to: $InstallDir"
Write-Host ''

# Manifest
New-Item -Path $InstallDir -ItemType Directory -Force | Out-Null
$Manifest = Join-Path $InstallDir '.installed-by-onebrain'
'' | Set-Content $Manifest -Encoding UTF8

# ── [1/4] Install files ─────────────────────────────────────
Write-Host '[1/4] Installing OneBrain...' -ForegroundColor Yellow
New-Item -Path $InstallBin -ItemType Directory -Force | Out-Null
New-Item -Path $InstallWeb -ItemType Directory -Force | Out-Null

Copy-Item "$ScriptDir\bin\onebrain.exe" "$InstallBin\onebrain.exe" -Force
Write-Host "  OK Binary: $InstallBin\onebrain.exe" -ForegroundColor Green

Copy-Item "$ScriptDir\web\*" $InstallWeb -Recurse -Force
Write-Host "  OK Web:    $InstallWeb" -ForegroundColor Green

# Launcher
$LauncherContent = @"
@echo off
title OneBrain Dashboard
where ollama >nul 2>nul
if %ERRORLEVEL% EQU 0 (
    curl -s http://localhost:11434/api/tags >nul 2>nul
    if %ERRORLEVEL% NEQ 0 (
        echo   Starting Ollama...
        start /B ollama serve >nul 2>nul
        timeout /t 3 >nul
    )
)
echo Starting OneBrain...
"$InstallBin\onebrain.exe" start --api --web-dir "$InstallWeb" %*
pause
"@
Set-Content -Path "$InstallDir\OneBrain Dashboard.bat" -Value $LauncherContent -Encoding UTF8
Write-Host "  OK Launcher: $InstallDir\OneBrain Dashboard.bat" -ForegroundColor Green
Write-Host ''

# ── [2/4] PATH ───────────────────────────────────────────────
Write-Host '[2/4] Configuring PATH...' -ForegroundColor Yellow
$CurrentPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if ($CurrentPath -notlike "*$InstallBin*") {
    [Environment]::SetEnvironmentVariable('Path', "$InstallBin;$CurrentPath", 'User')
    $env:Path = "$InstallBin;$env:Path"
    Write-Host "  OK Added to PATH" -ForegroundColor Green
    Write-Host '  Note: Restart terminal for PATH to take effect' -ForegroundColor DarkGray
} else {
    Write-Host '  OK Already in PATH' -ForegroundColor Green
}
Write-Host ''

# ── [3/4] Ollama ─────────────────────────────────────────────
Write-Host '[3/4] AI Engine (Ollama)...' -ForegroundColor Yellow
if (Get-Command ollama -ErrorAction SilentlyContinue) {
    Write-Host '  OK Ollama: already installed' -ForegroundColor Green
} else {
    Write-Host ''
    $InstallOllama = Read-Host '  Ollama not found. Install Ollama for AI features? [Y/n]'
    if (-not $InstallOllama -or $InstallOllama -match '^[Yy]') {
        $HasWinget = $false
        try { winget --version | Out-Null; $HasWinget = $true } catch {}
        if ($HasWinget) {
            winget install --id Ollama.Ollama -e --accept-source-agreements --accept-package-agreements --silent
        } else {
            Write-Host '    Downloading Ollama...'
            $OllamaExe = Join-Path $env:TEMP 'OllamaSetup.exe'
            Invoke-WebRequest -Uri 'https://ollama.ai/download/OllamaSetup.exe' -OutFile $OllamaExe -UseBasicParsing
            Start-Process $OllamaExe -ArgumentList '/VERYSILENT' -Wait
            Remove-Item $OllamaExe -ErrorAction SilentlyContinue
        }
        # Refresh PATH
        $env:Path = [System.Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' + [System.Environment]::GetEnvironmentVariable('Path', 'User')
        if (Get-Command ollama -ErrorAction SilentlyContinue) {
            Add-Content $Manifest 'ollama'
            Write-Host '  OK Ollama installed' -ForegroundColor Green
        } else {
            Write-Host '  ! Install failed - AI features unavailable' -ForegroundColor Yellow
        }
    } else {
        Write-Host '  - Skipped (AI features will be unavailable)' -ForegroundColor Yellow
    }
}
Write-Host ''

# ── [4/4] Pull AI model ─────────────────────────────────────
Write-Host '[4/4] AI Model...' -ForegroundColor Yellow
if (Get-Command ollama -ErrorAction SilentlyContinue) {
    $PullModel = Read-Host '  Download AI model (qwen3:8b, ~4.9GB)? [Y/n]'
    if (-not $PullModel -or $PullModel -match '^[Yy]') {
        try {
            $OllamaProc = Start-Process ollama -ArgumentList 'serve' -PassThru -WindowStyle Hidden -ErrorAction SilentlyContinue
            Start-Sleep -Seconds 3
            ollama pull qwen3:8b
            Write-Host '  OK Model ready' -ForegroundColor Green
            if ($OllamaProc -and -not $OllamaProc.HasExited) {
                $OllamaProc | Stop-Process -Force -ErrorAction SilentlyContinue
            }
        } catch {
            Write-Host "  ! Download failed - run 'ollama pull qwen3:8b' later" -ForegroundColor Yellow
        }
    } else {
        Write-Host "  - Skipped (run 'ollama pull qwen3:8b' later)"
    }
} else {
    Write-Host '  - Ollama not installed, skipping'
}

# ── Done! ────────────────────────────────────────────────────
Write-Host ''
Write-Host '================================================' -ForegroundColor Cyan
Write-Host '  OneBrain installed successfully!' -ForegroundColor Green
Write-Host ''
Write-Host '  Quick start:' -ForegroundColor White
Write-Host '    onebrain start --api' -ForegroundColor Yellow
Write-Host '    OR double-click:' -ForegroundColor White
Write-Host "    $InstallDir\OneBrain Dashboard.bat" -ForegroundColor Yellow
Write-Host ''
Write-Host '  Then open:' -ForegroundColor White
Write-Host '    http://localhost:4280' -ForegroundColor Yellow
Write-Host '    Token: onebrain-dev-token' -ForegroundColor Yellow
Write-Host ''
Write-Host '  Commands:' -ForegroundColor White
Write-Host '    onebrain start              # CLI only'
Write-Host '    onebrain start --api        # CLI + API + Web'
Write-Host '================================================' -ForegroundColor Cyan
'@
Set-Content -Path "$DistDir\install.ps1" -Value $InstallScript -Encoding UTF8

# ── Create uninstall.ps1 ─────────────────────────────────────
$UninstallScript = @'
# OneBrain Uninstaller
$ErrorActionPreference = 'Stop'

Write-Host 'OneBrain Uninstaller' -ForegroundColor Cyan
Write-Host ''

$InstallDir = Join-Path $env:LOCALAPPDATA 'OneBrain'
$InstallBin = Join-Path $InstallDir 'bin'
$Manifest = Join-Path $InstallDir '.installed-by-onebrain'

$Confirm = Read-Host 'Remove OneBrain? [Y/n]'
if ($Confirm -and $Confirm -notmatch '^[Yy]') { Write-Host 'Cancelled.'; exit 0 }

# Remove files
if (Test-Path $InstallDir) {
    # Read manifest before deleting
    $InstalledTools = @()
    if (Test-Path $Manifest) {
        $InstalledTools = Get-Content $Manifest | Where-Object { $_ -ne '' }
    }

    Remove-Item $InstallDir -Recurse -Force
    Write-Host "  OK Removed $InstallDir" -ForegroundColor Green
}

# Remove from PATH
$CurrentPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if ($CurrentPath -like "*$InstallBin*") {
    $NewPath = ($CurrentPath -split ';' | Where-Object { $_ -ne $InstallBin -and $_ -ne '' }) -join ';'
    [Environment]::SetEnvironmentVariable('Path', $NewPath, 'User')
    Write-Host '  OK Removed from PATH' -ForegroundColor Green
}

# Remove Ollama if we installed it
if ($InstalledTools -contains 'ollama') {
    $ConfirmOllama = Read-Host '  Remove Ollama (installed by OneBrain)? [y/N]'
    if ($ConfirmOllama -match '^[Yy]') {
        try { ollama rm qwen3:8b 2>$null } catch {}
        $HasWinget = $false
        try { winget --version | Out-Null; $HasWinget = $true } catch {}
        if ($HasWinget) {
            winget uninstall --id Ollama.Ollama -e --silent 2>$null
            Write-Host '  OK Ollama removed' -ForegroundColor Green
        } else {
            Write-Host '  - Uninstall Ollama from Control Panel' -ForegroundColor Yellow
        }
    } else {
        Write-Host '  - Ollama: kept'
    }
}

Write-Host ''
Write-Host 'OneBrain uninstalled.' -ForegroundColor Green
'@
Set-Content -Path "$DistDir\uninstall.ps1" -Value $UninstallScript -Encoding UTF8

# ── Create README ────────────────────────────────────────────
$ReadmeContent = @'
OneBrain — Decentralized Knowledge Network

INSTALL:
  Right-click install.ps1 → "Run with PowerShell"
  OR in PowerShell: .\install.ps1

AFTER INSTALL:
  onebrain start --api           # Start from terminal
  OR double-click "OneBrain Dashboard.bat"

  Open http://localhost:4280     # Web Dashboard
  Token: onebrain-dev-token

UNINSTALL:
  Right-click uninstall.ps1 → "Run with PowerShell"
  OR in PowerShell: .\uninstall.ps1

REQUIREMENTS:
  - Ollama (optional, for AI) — installer will ask to install
  - No other dependencies needed!
'@
Set-Content -Path "$DistDir\README.txt" -Value $ReadmeContent -Encoding UTF8

Write-Host "  OK Package contents ready" -ForegroundColor Green
Write-Host ''

# ── Create ZIP ───────────────────────────────────────────────
Write-Host '[5/5] Creating archive...' -ForegroundColor Yellow

$ZipPath = "$ReleaseDir\$PackageName.zip"
if (Test-Path $ZipPath) { Remove-Item $ZipPath -Force }
Compress-Archive -Path $DistDir -DestinationPath $ZipPath

$ZipSize = '{0:N1} MB' -f ((Get-Item $ZipPath).Length / 1MB)
$BinSize = '{0:N1} MB' -f ((Get-Item "$DistDir\bin\onebrain.exe").Length / 1MB)
$WebFiles = (Get-ChildItem "$DistDir\web" -Recurse -File).Count

Write-Host ''
Write-Host '================================================' -ForegroundColor Cyan
Write-Host 'Release package created!' -ForegroundColor Green
Write-Host ''
Write-Host "  $ZipPath ($ZipSize)" -ForegroundColor White
Write-Host ''
Write-Host '  Contents:' -ForegroundColor White
Write-Host "    bin\onebrain.exe  - CLI + API server ($BinSize)"
Write-Host "    web\              - Web Dashboard ($WebFiles files)"
Write-Host '    install.ps1       - User installer'
Write-Host '    uninstall.ps1     - User uninstaller'
Write-Host '    README.txt        - Quick start guide'
Write-Host ''
Write-Host '  Send this ZIP to users. They extract and run:' -ForegroundColor White
Write-Host '    install.ps1 (right-click → Run with PowerShell)' -ForegroundColor Yellow
Write-Host '================================================' -ForegroundColor Cyan
