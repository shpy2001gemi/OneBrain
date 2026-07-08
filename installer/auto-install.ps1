# ══════════════════════════════════════════════════════════════
# OneBrain — Auto Installer (Windows)
#
# Usage (PowerShell):
#   irm https://raw.githubusercontent.com/<org>/OneBrain/main/installer/auto-install.ps1 | iex
#   OR
#   .\auto-install.ps1
#
# What this script does:
#   1. Install Rust, Node.js, Ollama (if missing)
#   2. Clone OneBrain repository
#   3. Build CLI + Web Dashboard
#   4. Install to %LOCALAPPDATA%\OneBrain
#   5. Add to PATH
#   6. Pull default AI model (qwen3:8b)
#   7. Ready to run!
# ══════════════════════════════════════════════════════════════

$ErrorActionPreference = 'Stop'

$RepoUrl = 'https://github.com/<your-org>/OneBrain.git'
$DefaultModel = 'qwen3:8b'
$Branch = 'main'

# ── Helpers ──────────────────────────────────────────────────
function Test-Winget {
    try { winget --version | Out-Null; return $true }
    catch { return $false }
}

function Refresh-Path {
    $env:Path = [System.Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' +
                [System.Environment]::GetEnvironmentVariable('Path', 'User')
    # Also add cargo
    $CargoPath = Join-Path $env:USERPROFILE '.cargo\bin'
    if (Test-Path $CargoPath) { $env:Path = "$CargoPath;$env:Path" }
}

Write-Host ''
Write-Host '  +================================================+' -ForegroundColor Cyan
Write-Host '  |   OneBrain — Auto Installer (Windows)          |' -ForegroundColor Cyan
Write-Host '  |   Decentralized Knowledge Network              |' -ForegroundColor Cyan
Write-Host '  +================================================+' -ForegroundColor Cyan
Write-Host ''

$HasWinget = Test-Winget
if ($HasWinget) {
    Write-Host '  Package manager: winget' -ForegroundColor DarkGray
} else {
    Write-Host '  Package manager: direct download' -ForegroundColor DarkGray
}
Write-Host ''

# ══════════════════════════════════════════════════════════════
Write-Host '[1/7] Installing prerequisites...' -ForegroundColor Cyan
Write-Host ''
# ══════════════════════════════════════════════════════════════

# Manifest: track what WE install (so uninstaller knows)
$ManifestDir = Join-Path $env:LOCALAPPDATA 'OneBrain'
New-Item -Path $ManifestDir -ItemType Directory -Force | Out-Null
$Manifest = Join-Path $ManifestDir '.installed-by-onebrain'
# Start fresh manifest
'' | Set-Content $Manifest -Encoding UTF8

# ── Git (required for clone) ──────────────────────────────────
if (Get-Command git -ErrorAction SilentlyContinue) {
    Write-Host "  OK Git: $(git --version)" -ForegroundColor Green
} else {
    $InstallGit = Read-Host '  ! Git not found. Install Git? [Y/n]'
    if (-not $InstallGit -or $InstallGit -match '^[Yy]') {
        if ($HasWinget) {
            winget install --id Git.Git -e --accept-source-agreements --accept-package-agreements --silent
        } else {
            $GitUrl = 'https://github.com/git-for-windows/git/releases/download/v2.47.0.windows.1/Git-2.47.0-64-bit.exe'
            $GitExe = Join-Path $env:TEMP 'git-installer.exe'
            Invoke-WebRequest -Uri $GitUrl -OutFile $GitExe -UseBasicParsing
            Start-Process $GitExe -ArgumentList '/VERYSILENT /NORESTART' -Wait
            Remove-Item $GitExe -ErrorAction SilentlyContinue
        }
        Refresh-Path
        if (Get-Command git -ErrorAction SilentlyContinue) {
            Write-Host "  OK Git installed: $(git --version)" -ForegroundColor Green
            Add-Content $Manifest 'git'
        } else {
            Write-Host '  X Git install failed. Install manually: https://git-scm.com' -ForegroundColor Red
            exit 1
        }
    } else {
        Write-Host '  X Git is required to continue.' -ForegroundColor Red
        exit 1
    }
}

# ── Rust (required for build) ────────────────────────────────
if (Get-Command cargo -ErrorAction SilentlyContinue) {
    Write-Host "  OK Rust: $(rustc --version)" -ForegroundColor Green
} else {
    $InstallRust = Read-Host '  ! Rust not found. Install Rust (via rustup)? [Y/n]'
    if (-not $InstallRust -or $InstallRust -match '^[Yy]') {
        if ($HasWinget) {
            winget install --id Rustlang.Rustup -e --accept-source-agreements --accept-package-agreements --silent
        } else {
            $RustupExe = Join-Path $env:TEMP 'rustup-init.exe'
            Invoke-WebRequest -Uri 'https://win.rustup.rs/x86_64' -OutFile $RustupExe -UseBasicParsing
            & $RustupExe -y --default-toolchain stable
            Remove-Item $RustupExe -ErrorAction SilentlyContinue
        }
        Refresh-Path
        if (Get-Command cargo -ErrorAction SilentlyContinue) {
            Write-Host "  OK Rust installed: $(rustc --version)" -ForegroundColor Green
            Add-Content $Manifest 'rust'
        } else {
            Write-Host '  X Rust install failed. Restart terminal and re-run.' -ForegroundColor Red
            exit 1
        }
    } else {
        Write-Host '  X Rust is required to build OneBrain.' -ForegroundColor Red
        exit 1
    }
}

# ── Node.js (required for web dashboard) ─────────────────────
if (Get-Command node -ErrorAction SilentlyContinue) {
    Write-Host "  OK Node.js: $(node --version)" -ForegroundColor Green
} else {
    $InstallNode = Read-Host '  ! Node.js not found. Install Node.js? [Y/n]'
    if (-not $InstallNode -or $InstallNode -match '^[Yy]') {
        if ($HasWinget) {
            winget install --id OpenJS.NodeJS.LTS -e --accept-source-agreements --accept-package-agreements --silent
        } else {
            $NodeUrl = 'https://nodejs.org/dist/v20.18.0/node-v20.18.0-x64.msi'
            $NodeMsi = Join-Path $env:TEMP 'node-installer.msi'
            Invoke-WebRequest -Uri $NodeUrl -OutFile $NodeMsi -UseBasicParsing
            Start-Process msiexec.exe -ArgumentList "/i `"$NodeMsi`" /qn" -Wait
            Remove-Item $NodeMsi -ErrorAction SilentlyContinue
        }
        Refresh-Path
        if (Get-Command node -ErrorAction SilentlyContinue) {
            Write-Host "  OK Node.js installed: $(node --version)" -ForegroundColor Green
            Add-Content $Manifest 'nodejs'
        } else {
            Write-Host '  X Node.js install failed. Install manually: https://nodejs.org' -ForegroundColor Red
            exit 1
        }
    } else {
        Write-Host '  X Node.js is required to build Web Dashboard.' -ForegroundColor Red
        exit 1
    }
}

# ── Ollama (optional, for AI features) ───────────────────────
if (Get-Command ollama -ErrorAction SilentlyContinue) {
    Write-Host '  OK Ollama: installed' -ForegroundColor Green
} else {
    Write-Host ''
    $InstallOllama = Read-Host '  ! Ollama not found. Install Ollama for AI features? [Y/n]'
    if (-not $InstallOllama -or $InstallOllama -match '^[Yy]') {
        if ($HasWinget) {
            winget install --id Ollama.Ollama -e --accept-source-agreements --accept-package-agreements --silent
        } else {
            $OllamaExe = Join-Path $env:TEMP 'OllamaSetup.exe'
            Invoke-WebRequest -Uri 'https://ollama.ai/download/OllamaSetup.exe' -OutFile $OllamaExe -UseBasicParsing
            Start-Process $OllamaExe -ArgumentList '/VERYSILENT' -Wait
            Remove-Item $OllamaExe -ErrorAction SilentlyContinue
        }
        Refresh-Path
        if (Get-Command ollama -ErrorAction SilentlyContinue) {
            Write-Host '  OK Ollama installed' -ForegroundColor Green
            Add-Content $Manifest 'ollama'
        } else {
            Write-Host '  ! Ollama install failed - AI features unavailable' -ForegroundColor Yellow
        }
    } else {
        Write-Host '  - Skipped Ollama - AI features (Chat, Encode) will be unavailable' -ForegroundColor Yellow
    }
}

Write-Host ''

# ══════════════════════════════════════════════════════════════
Write-Host '[2/7] Cloning OneBrain...' -ForegroundColor Cyan
# ══════════════════════════════════════════════════════════════

$InstallTmp = Join-Path $env:TEMP 'onebrain-install'

if (Test-Path (Join-Path $InstallTmp '.git')) {
    Write-Host '  Repository exists - pulling latest...'
    Push-Location $InstallTmp
    git pull --ff-only origin $Branch 2>$null
    Pop-Location
} else {
    if (Test-Path $InstallTmp) { Remove-Item $InstallTmp -Recurse -Force }
    git clone --depth 1 --branch $Branch $RepoUrl $InstallTmp
}
Write-Host "  OK Cloned to $InstallTmp" -ForegroundColor Green
Write-Host ''

# ══════════════════════════════════════════════════════════════
Write-Host '[3/7] Building CLI (Rust, release mode)...' -ForegroundColor Cyan
# ══════════════════════════════════════════════════════════════

Push-Location (Join-Path $InstallTmp 'src')
cargo build --release -p onebrain-cli
if ($LASTEXITCODE -ne 0) { Write-Host '  X Build failed' -ForegroundColor Red; exit 1 }
Pop-Location
Write-Host '  OK CLI binary built' -ForegroundColor Green
Write-Host ''

# ══════════════════════════════════════════════════════════════
Write-Host '[4/7] Building Web Dashboard (React/Vite)...' -ForegroundColor Cyan
# ══════════════════════════════════════════════════════════════

Push-Location (Join-Path $InstallTmp 'src\onebrain-web')
if (-not (Test-Path 'node_modules')) { npm install --silent }
npm run build
if ($LASTEXITCODE -ne 0) { Write-Host '  X Web build failed' -ForegroundColor Red; exit 1 }
Pop-Location
Write-Host '  OK Web Dashboard built' -ForegroundColor Green
Write-Host ''

# ══════════════════════════════════════════════════════════════
Write-Host '[5/7] Installing...' -ForegroundColor Cyan
# ══════════════════════════════════════════════════════════════

$InstallDir = Join-Path $env:LOCALAPPDATA 'OneBrain'
$InstallBin = Join-Path $InstallDir 'bin'
$InstallWeb = Join-Path $InstallDir 'web'

New-Item -Path $InstallBin -ItemType Directory -Force | Out-Null
New-Item -Path $InstallWeb -ItemType Directory -Force | Out-Null

# Binary
Copy-Item (Join-Path $InstallTmp 'src\target\release\onebrain.exe') "$InstallBin\onebrain.exe" -Force
Write-Host "  OK Binary: $InstallBin\onebrain.exe" -ForegroundColor Green

# Web
Copy-Item (Join-Path $InstallTmp 'src\onebrain-web\dist\*') $InstallWeb -Recurse -Force
Write-Host "  OK Web:    $InstallWeb" -ForegroundColor Green

# Launcher .bat
$LauncherContent = @"
@echo off
title OneBrain Dashboard
:: Auto-start Ollama
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

# PATH
$CurrentPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if ($CurrentPath -notlike "*$InstallBin*") {
    [Environment]::SetEnvironmentVariable('Path', "$InstallBin;$CurrentPath", 'User')
    $env:Path = "$InstallBin;$env:Path"
    Write-Host "  OK Added to PATH" -ForegroundColor Green
} else {
    Write-Host '  OK Already in PATH' -ForegroundColor Green
}
Write-Host ''

# ══════════════════════════════════════════════════════════════
Write-Host '[6/7] Pulling AI model...' -ForegroundColor Cyan
# ══════════════════════════════════════════════════════════════

if (Get-Command ollama -ErrorAction SilentlyContinue) {
    Write-Host "  Downloading $DefaultModel (this may take a few minutes)..."
    try {
        $OllamaProc = Start-Process ollama -ArgumentList 'serve' -PassThru -WindowStyle Hidden -ErrorAction SilentlyContinue
        Start-Sleep -Seconds 3
        ollama pull $DefaultModel
        Write-Host "  OK Model $DefaultModel ready" -ForegroundColor Green
        if ($OllamaProc -and -not $OllamaProc.HasExited) {
            $OllamaProc | Stop-Process -Force -ErrorAction SilentlyContinue
        }
    } catch {
        Write-Host "  ! Model pull failed - run 'ollama pull $DefaultModel' later" -ForegroundColor Yellow
    }
} else {
    Write-Host '  ! Ollama not available - skipping' -ForegroundColor Yellow
}
Write-Host ''

# ══════════════════════════════════════════════════════════════
Write-Host '[7/7] Cleaning up...' -ForegroundColor Cyan
# ══════════════════════════════════════════════════════════════

# Remove build artifacts (keep source for updates)
Remove-Item (Join-Path $InstallTmp 'src\target') -Recurse -Force -ErrorAction SilentlyContinue
Write-Host '  OK Cleaned build cache' -ForegroundColor Green
Write-Host ''

# ══════════════════════════════════════════════════════════════
# DONE!
# ══════════════════════════════════════════════════════════════
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
Write-Host ''
Write-Host '  Note: Restart terminal for PATH to take effect' -ForegroundColor DarkGray
Write-Host '================================================' -ForegroundColor Cyan
