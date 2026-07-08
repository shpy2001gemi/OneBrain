# OneBrain Build Script — Windows
# Builds both the Rust CLI binary and the Web Dashboard
# Auto-installs missing prerequisites (Rust, Node.js, Ollama)

$ErrorActionPreference = 'Stop'

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectRoot = Split-Path -Parent $ScriptDir
$SrcDir = Join-Path $ProjectRoot 'src'
$BuildDir = Join-Path $ProjectRoot 'build'

Write-Host ''
Write-Host '  +================================================+' -ForegroundColor Cyan
Write-Host '  |       OneBrain — Build System (Windows)        |' -ForegroundColor Cyan
Write-Host '  +================================================+' -ForegroundColor Cyan
Write-Host ''

# ══════════════════════════════════════════════════════════════
# Helper: Check if winget is available
# ══════════════════════════════════════════════════════════════
function Test-Winget {
    try { winget --version | Out-Null; return $true }
    catch { return $false }
}

# Helper: Refresh PATH in current session
function Refresh-Path {
    $env:Path = [System.Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' +
                [System.Environment]::GetEnvironmentVariable('Path', 'User')
}

$InstalledSomething = $false

# ══════════════════════════════════════════════════════════════
# [1/6] Check & Install Prerequisites
# ══════════════════════════════════════════════════════════════
Write-Host '[1/6] Checking & installing prerequisites...' -ForegroundColor Yellow
Write-Host ''

$HasWinget = Test-Winget
if ($HasWinget) {
    Write-Host '  Package manager: winget' -ForegroundColor DarkGray
} else {
    Write-Host '  Package manager: manual install (winget not found)' -ForegroundColor DarkGray
}
Write-Host ''

# ── 1a. Rust ─────────────────────────────────────────────────
if (Get-Command cargo -ErrorAction SilentlyContinue) {
    $RustVer = (rustc --version)
    Write-Host "  OK Rust: $RustVer" -ForegroundColor Green
} else {
    Write-Host '  ! Rust not found - installing via rustup...' -ForegroundColor Yellow

    if ($HasWinget) {
        Write-Host '    Installing via winget...'
        winget install --id Rustlang.Rustup -e --accept-source-agreements --accept-package-agreements --silent
    } else {
        Write-Host '    Downloading rustup-init.exe...'
        $RustupUrl = 'https://win.rustup.rs/x86_64'
        $RustupExe = Join-Path $env:TEMP 'rustup-init.exe'
        Invoke-WebRequest -Uri $RustupUrl -OutFile $RustupExe -UseBasicParsing
        & $RustupExe -y --default-toolchain stable
        Remove-Item $RustupExe -ErrorAction SilentlyContinue
    }

    # Add cargo to PATH for this session
    $CargoPath = Join-Path $env:USERPROFILE '.cargo\bin'
    if ($env:Path -notlike "*$CargoPath*") {
        $env:Path = "$CargoPath;$env:Path"
    }
    Refresh-Path

    if (Get-Command cargo -ErrorAction SilentlyContinue) {
        $RustVer = (rustc --version)
        Write-Host "  OK Rust installed: $RustVer" -ForegroundColor Green
        $InstalledSomething = $true
    } else {
        Write-Host '  X Rust installation failed.' -ForegroundColor Red
        Write-Host '    Install manually: https://rustup.rs' -ForegroundColor Red
        Write-Host '    Then restart this terminal and re-run.' -ForegroundColor Red
        exit 1
    }
}

# ── 1b. Node.js ──────────────────────────────────────────────
if (Get-Command node -ErrorAction SilentlyContinue) {
    $NodeVer = (node --version)
    Write-Host "  OK Node.js: $NodeVer" -ForegroundColor Green
} else {
    Write-Host '  ! Node.js not found - installing...' -ForegroundColor Yellow

    if ($HasWinget) {
        Write-Host '    Installing via winget...'
        winget install --id OpenJS.NodeJS.LTS -e --accept-source-agreements --accept-package-agreements --silent
    } else {
        Write-Host '    Downloading Node.js installer...'
        $NodeUrl = 'https://nodejs.org/dist/v20.18.0/node-v20.18.0-x64.msi'
        $NodeMsi = Join-Path $env:TEMP 'node-installer.msi'
        Invoke-WebRequest -Uri $NodeUrl -OutFile $NodeMsi -UseBasicParsing
        Write-Host '    Running installer (this may take a minute)...'
        Start-Process msiexec.exe -ArgumentList "/i `"$NodeMsi`" /qn" -Wait
        Remove-Item $NodeMsi -ErrorAction SilentlyContinue
    }

    Refresh-Path

    if (Get-Command node -ErrorAction SilentlyContinue) {
        $NodeVer = (node --version)
        Write-Host "  OK Node.js installed: $NodeVer" -ForegroundColor Green
        $InstalledSomething = $true
    } else {
        Write-Host '  X Node.js installation failed.' -ForegroundColor Red
        Write-Host '    Install manually: https://nodejs.org' -ForegroundColor Red
        Write-Host '    Then restart this terminal and re-run.' -ForegroundColor Red
        exit 1
    }
}

# ── 1c. Ollama (optional) ────────────────────────────────────
if (Get-Command ollama -ErrorAction SilentlyContinue) {
    try {
        $OllamaVer = (ollama --version 2>$null)
        Write-Host "  OK Ollama: $OllamaVer" -ForegroundColor Green
    } catch {
        Write-Host '  OK Ollama: installed' -ForegroundColor Green
    }
} else {
    Write-Host '  ! Ollama not found - installing (for AI features)...' -ForegroundColor Yellow

    if ($HasWinget) {
        Write-Host '    Installing via winget...'
        winget install --id Ollama.Ollama -e --accept-source-agreements --accept-package-agreements --silent
    } else {
        Write-Host '    Downloading Ollama installer...'
        $OllamaUrl = 'https://ollama.ai/download/OllamaSetup.exe'
        $OllamaExe = Join-Path $env:TEMP 'OllamaSetup.exe'
        Invoke-WebRequest -Uri $OllamaUrl -OutFile $OllamaExe -UseBasicParsing
        Write-Host '    Running installer...'
        Start-Process $OllamaExe -ArgumentList '/VERYSILENT' -Wait
        Remove-Item $OllamaExe -ErrorAction SilentlyContinue
    }

    Refresh-Path

    if (Get-Command ollama -ErrorAction SilentlyContinue) {
        Write-Host '  OK Ollama installed' -ForegroundColor Green
        $InstalledSomething = $true

        # Pull default model
        Write-Host ''
        Write-Host '  Pulling default AI model (qwen3:8b)...' -ForegroundColor Yellow
        Write-Host '  This may take a few minutes.'
        try {
            # Start ollama serve if not running
            $OllamaProc = Start-Process ollama -ArgumentList 'serve' -PassThru -WindowStyle Hidden -ErrorAction SilentlyContinue
            Start-Sleep -Seconds 3
            ollama pull qwen3:8b
            if ($OllamaProc -and -not $OllamaProc.HasExited) {
                $OllamaProc | Stop-Process -Force -ErrorAction SilentlyContinue
            }
        } catch {
            Write-Host "  ! Model pull failed - run 'ollama pull qwen3:8b' later" -ForegroundColor Yellow
        }
    } else {
        Write-Host '  ! Ollama install failed - AI features unavailable' -ForegroundColor Yellow
        Write-Host '    Install manually: https://ollama.ai' -ForegroundColor Yellow
    }
}

Write-Host ''

if ($InstalledSomething) {
    Write-Host '  ----------------------------------------' -ForegroundColor DarkGray
    Write-Host '  New software was installed. If you see' -ForegroundColor DarkGray
    Write-Host '  errors below, restart terminal and re-run.' -ForegroundColor DarkGray
    Write-Host '  ----------------------------------------' -ForegroundColor DarkGray
    Write-Host ''
}

# ══════════════════════════════════════════════════════════════
# [2/6] Build Rust CLI
# ══════════════════════════════════════════════════════════════
Write-Host '[2/6] Building OneBrain CLI (Rust, release mode)...' -ForegroundColor Yellow
Push-Location $SrcDir
cargo build --release -p onebrain-cli
if ($LASTEXITCODE -ne 0) { Write-Host '  X Rust build failed' -ForegroundColor Red; exit 1 }
Pop-Location
Write-Host '  OK CLI binary built' -ForegroundColor Green
Write-Host ''

# ══════════════════════════════════════════════════════════════
# [3/6] Build Web Dashboard
# ══════════════════════════════════════════════════════════════
Write-Host '[3/6] Building Web Dashboard (React/Vite)...' -ForegroundColor Yellow
Push-Location (Join-Path $SrcDir 'onebrain-web')
if (-not (Test-Path 'node_modules')) {
    Write-Host '  Installing npm dependencies...'
    npm install --silent
}
npm run build
if ($LASTEXITCODE -ne 0) { Write-Host '  X Web build failed' -ForegroundColor Red; exit 1 }
Pop-Location
Write-Host '  OK Web Dashboard built' -ForegroundColor Green
Write-Host ''

# ══════════════════════════════════════════════════════════════
# [4/6] Create distribution
# ══════════════════════════════════════════════════════════════
Write-Host '[4/6] Creating distribution package...' -ForegroundColor Yellow
if (Test-Path $BuildDir) { Remove-Item $BuildDir -Recurse -Force }
New-Item -Path "$BuildDir\bin" -ItemType Directory -Force | Out-Null
New-Item -Path "$BuildDir\web" -ItemType Directory -Force | Out-Null

Copy-Item "$SrcDir\target\release\onebrain.exe" "$BuildDir\bin\onebrain.exe"
Copy-Item "$SrcDir\onebrain-web\dist\*" "$BuildDir\web\" -Recurse

Write-Host "  OK Distribution created at: $BuildDir" -ForegroundColor Green
Write-Host ''

# ══════════════════════════════════════════════════════════════
# [5/6] Create launcher
# ══════════════════════════════════════════════════════════════
Write-Host '[5/6] Creating launcher...' -ForegroundColor Yellow

$LauncherContent = @'
@echo off
title OneBrain
set SCRIPT_DIR=%~dp0
set BIN=%SCRIPT_DIR%bin\onebrain.exe
set WEB_DIR=%SCRIPT_DIR%web

if not exist "%BIN%" (
    echo OneBrain binary not found at %BIN%
    pause
    exit /b 1
)

:: Start Ollama if available and not running
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
echo.
"%BIN%" start --api --web-dir "%WEB_DIR%" %*
pause
'@

Set-Content -Path "$BuildDir\start.bat" -Value $LauncherContent -Encoding UTF8

Write-Host '  OK Launcher created' -ForegroundColor Green
Write-Host ''

# ══════════════════════════════════════════════════════════════
# [6/6] Verify
# ══════════════════════════════════════════════════════════════
Write-Host '[6/6] Verifying...' -ForegroundColor Yellow
$BinSize = (Get-Item "$BuildDir\bin\onebrain.exe").Length / 1MB
$BinSizeStr = '{0:N1} MB' -f $BinSize
$WebFiles = (Get-ChildItem "$BuildDir\web" -Recurse -File).Count
Write-Host "  OK Binary: $BinSizeStr" -ForegroundColor Green
Write-Host "  OK Web Dashboard: $WebFiles files" -ForegroundColor Green
Write-Host ''

# ── Summary ──────────────────────────────────────────────────
Write-Host '================================================' -ForegroundColor Cyan
Write-Host 'BUILD COMPLETE!' -ForegroundColor Green
Write-Host ''
Write-Host "Distribution: $BuildDir" -ForegroundColor White
Write-Host '  bin\onebrain.exe  - CLI + API server'
Write-Host '  web\              - Web Dashboard (static)'
Write-Host '  start.bat         - Quick launcher (auto-starts Ollama)'
Write-Host ''
Write-Host "To run:   cd $BuildDir && .\start.bat" -ForegroundColor Yellow
Write-Host 'Browser:  http://localhost:4280' -ForegroundColor Yellow
Write-Host '================================================' -ForegroundColor Cyan
