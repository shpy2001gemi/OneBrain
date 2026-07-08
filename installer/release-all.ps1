# ══════════════════════════════════════════════════════════════
# OneBrain — Build Release Packages for ALL Platforms
#
# Creates 3 distributable packages from Windows:
#   1. onebrain-windows-x64.zip       (native build)
#   2. onebrain-linux-x86_64.tar.gz   (cross-compile)
#   3. onebrain-macos-arm64.tar.gz    (cross-compile)
#
# Prerequisites: Rust with cross targets, Node.js
# ══════════════════════════════════════════════════════════════

$ErrorActionPreference = 'Continue'

$ProjectRoot = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$SrcDir = Join-Path $ProjectRoot 'src'
$ReleaseDir = Join-Path $ProjectRoot 'release'
$WebDir = Join-Path $SrcDir 'onebrain-web'
$InstallerDir = Join-Path $ProjectRoot 'installer'

Write-Host ''
Write-Host '  +================================================+' -ForegroundColor Cyan
Write-Host '  |   OneBrain — Multi-Platform Release Builder    |' -ForegroundColor Cyan
Write-Host '  +================================================+' -ForegroundColor Cyan
Write-Host ''

# ── Cleanup ──────────────────────────────────────────────────
if (Test-Path $ReleaseDir) { Remove-Item $ReleaseDir -Recurse -Force }
New-Item -Path $ReleaseDir -ItemType Directory -Force | Out-Null

# ══════════════════════════════════════════════════════════════
# Step 1: Build Web Dashboard (shared across all platforms)
# ══════════════════════════════════════════════════════════════
Write-Host '[1/5] Building Web Dashboard...' -ForegroundColor Yellow
Push-Location $WebDir
if (-not (Test-Path 'node_modules')) { npm install --silent }
npm run build 2>&1 | Select-Object -Last 3
Pop-Location
Write-Host '  OK Web dashboard built' -ForegroundColor Green
Write-Host ''

# ══════════════════════════════════════════════════════════════
# Step 2: Build Windows binary (native)
# ══════════════════════════════════════════════════════════════
Write-Host '[2/5] Building Windows binary (native)...' -ForegroundColor Yellow
Push-Location $SrcDir
cmd /c "cargo build --release -p onebrain-cli 2>&1" | Select-Object -Last 5
if (-not (Test-Path 'target\release\onebrain.exe')) {
    Write-Host '  X Windows build failed!' -ForegroundColor Red; exit 1
}
Pop-Location
Write-Host '  OK Windows binary built' -ForegroundColor Green
Write-Host ''

# ══════════════════════════════════════════════════════════════
# Step 3: Cross-compile for Linux & macOS (best-effort)
# ══════════════════════════════════════════════════════════════
Write-Host '[3/5] Cross-compiling for Linux and macOS...' -ForegroundColor Yellow

$CrossTargets = @(
    @{ Name = 'linux-x86_64';  Target = 'x86_64-unknown-linux-gnu';  Ext = '' }
    @{ Name = 'macos-arm64';   Target = 'aarch64-apple-darwin';      Ext = '' }
)

$BuiltTargets = @()
$FailedTargets = @()

foreach ($T in $CrossTargets) {
    Write-Host "  Building $($T.Name)..." -ForegroundColor DarkGray
    Push-Location $SrcDir
    $BuildResult = cmd /c "cargo build --release -p onebrain-cli --target $($T.Target) 2>&1"
    $BinaryPath = "target\$($T.Target)\release\onebrain$($T.Ext)"
    if (Test-Path $BinaryPath) {
        $BuiltTargets += $T
        Write-Host "  OK $($T.Name)" -ForegroundColor Green
    } else {
        $FailedTargets += $T
        Write-Host "  ! $($T.Name): cross-compile failed (needs linker)" -ForegroundColor Yellow
        Write-Host "    Run release.sh on $($T.Name) instead" -ForegroundColor DarkGray
    }
    Pop-Location
}
Write-Host ''

# ══════════════════════════════════════════════════════════════
# Step 4: Package all platforms
# ══════════════════════════════════════════════════════════════
Write-Host '[4/5] Creating release packages...' -ForegroundColor Yellow

# ── Helper function ──────────────────────────────────────────
function New-ReleasePackage {
    param(
        [string]$PkgName,
        [string]$BinarySource,
        [string]$BinaryDest,
        [bool]$IsWindows
    )
    
    $PkgDir = Join-Path $ReleaseDir $PkgName
    New-Item -Path "$PkgDir\bin" -ItemType Directory -Force | Out-Null
    New-Item -Path "$PkgDir\web" -ItemType Directory -Force | Out-Null
    
    # Copy binary
    Copy-Item $BinarySource "$PkgDir\bin\$BinaryDest" -Force
    
    # Copy web
    Copy-Item "$WebDir\dist\*" "$PkgDir\web\" -Recurse -Force
    
    if ($IsWindows) {
        # Copy install/uninstall scripts from release.ps1 embedded content
        # We'll create them inline
        Create-WindowsInstallerScripts -PkgDir $PkgDir
        
        # README
        @"
OneBrain — Decentralized Knowledge Network

INSTALL:
  Right-click install.ps1 → "Run with PowerShell"

AFTER INSTALL:
  onebrain start --api
  OR double-click "OneBrain Dashboard.bat"
  Open http://localhost:4280
  Token: onebrain-dev-token

UNINSTALL:
  Right-click uninstall.ps1 → "Run with PowerShell"
"@ | Set-Content "$PkgDir\README.txt" -Encoding UTF8

        # Create ZIP
        $ZipPath = "$ReleaseDir\$PkgName.zip"
        if (Test-Path $ZipPath) { Remove-Item $ZipPath }
        Compress-Archive -Path $PkgDir -DestinationPath $ZipPath
        return $ZipPath
    } else {
        # Copy install/uninstall scripts
        Create-UnixInstallerScripts -PkgDir $PkgDir
        
        @"
OneBrain — Decentralized Knowledge Network

INSTALL:
  chmod +x install.sh
  ./install.sh

AFTER INSTALL:
  onebrain-dashboard
  Open http://localhost:4280
  Token: onebrain-dev-token

UNINSTALL:
  chmod +x uninstall.sh
  ./uninstall.sh
"@ | Set-Content "$PkgDir\README.txt" -Encoding UTF8

        # Create tar.gz (use tar if available, else zip)
        $TarPath = "$ReleaseDir\$PkgName.tar.gz"
        try {
            Push-Location $ReleaseDir
            tar -czf "$PkgName.tar.gz" $PkgName
            Pop-Location
            return $TarPath
        } catch {
            $ZipPath = "$ReleaseDir\$PkgName.zip"
            Compress-Archive -Path $PkgDir -DestinationPath $ZipPath
            return $ZipPath
        }
    }
}

function Create-WindowsInstallerScripts {
    param([string]$PkgDir)
    
    # Read from release.ps1 generated content or create inline
    $InstallContent = @'
$ErrorActionPreference = 'Stop'
Write-Host ''
Write-Host '  +================================================+' -ForegroundColor Cyan
Write-Host '  |   OneBrain — Installer                         |' -ForegroundColor Cyan
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
'@
    Set-Content -Path "$PkgDir\install.ps1" -Value $InstallContent -Encoding UTF8

    $UninstallContent = @'
$ErrorActionPreference = 'Stop'
Write-Host 'OneBrain Uninstaller' -ForegroundColor Cyan
$InstallDir = Join-Path $env:LOCALAPPDATA 'OneBrain'
$InstallBin = Join-Path $InstallDir 'bin'
$Manifest = Join-Path $InstallDir '.installed-by-onebrain'
$C = Read-Host 'Remove OneBrain? [Y/n]'
if ($C -and $C -notmatch '^[Yy]') { exit 0 }
$Tools = @()
if (Test-Path $Manifest) { $Tools = Get-Content $Manifest | Where-Object { $_ -ne '' } }
if (Test-Path $InstallDir) { Remove-Item $InstallDir -Recurse -Force; Write-Host '  OK Removed' -ForegroundColor Green }
$P = [Environment]::GetEnvironmentVariable('Path','User')
if ($P -like "*$InstallBin*") {
    $NP = ($P -split ';' | Where-Object { $_ -ne $InstallBin -and $_ -ne '' }) -join ';'
    [Environment]::SetEnvironmentVariable('Path',$NP,'User')
    Write-Host '  OK PATH cleaned' -ForegroundColor Green
}
if ($Tools -contains 'ollama') {
    $R = Read-Host '  Remove Ollama (installed by OneBrain)? [y/N]'
    if ($R -match '^[Yy]') {
        try { ollama rm qwen3:8b 2>$null } catch {}
        $W = $false; try { winget --version | Out-Null; $W = $true } catch {}
        if ($W) { winget uninstall --id Ollama.Ollama -e --silent 2>$null }
        Write-Host '  OK Ollama removed' -ForegroundColor Green
    }
}
Write-Host 'Done.' -ForegroundColor Green
Read-Host 'Press Enter to close'
'@
    Set-Content -Path "$PkgDir\uninstall.ps1" -Value $UninstallContent -Encoding UTF8
}

function Create-UnixInstallerScripts {
    param([string]$PkgDir)
    
    $InstallContent = @'
#!/usr/bin/env bash
set -euo pipefail
echo ""
echo "  ╔══════════════════════════════════════════════╗"
echo "  ║   🧠 OneBrain — Installer                   ║"
echo "  ╚══════════════════════════════════════════════╝"
echo ""
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
[ ! -f "$SCRIPT_DIR/bin/onebrain" ] && echo "❌ Package incomplete" && exit 1

if [ "$(id -u)" -eq 0 ]; then PREFIX="/usr/local"; else PREFIX="$HOME/.local"; fi
BIN="$PREFIX/bin"; SHARE="$PREFIX/share/onebrain"
mkdir -p "$BIN" "$SHARE/web"
MANIFEST="$SHARE/.installed-by-onebrain"; > "$MANIFEST"

echo "[1/4] Installing..."
cp "$SCRIPT_DIR/bin/onebrain" "$BIN/onebrain"; chmod +x "$BIN/onebrain"
cp -r "$SCRIPT_DIR/web/"* "$SHARE/web/"
cat > "$BIN/onebrain-dashboard" << EOF
#!/usr/bin/env bash
command -v ollama >/dev/null 2>&1 && { curl -s http://localhost:11434/api/tags >/dev/null 2>&1 || { ollama serve >/dev/null 2>&1 & sleep 2; }; }
exec "$BIN/onebrain" start --api --web-dir "$SHARE/web" "\$@"
EOF
chmod +x "$BIN/onebrain-dashboard"
echo "  ✓ Installed to $PREFIX"

echo "[2/4] PATH..."
if ! echo "$PATH" | grep -q "$BIN"; then
    SHELL_NAME=$(basename "${SHELL:-bash}")
    case "$SHELL_NAME" in
        zsh)  RC="$HOME/.zshrc" ;;
        fish) RC="$HOME/.config/fish/config.fish" ;;
        *)    RC="$HOME/.bashrc" ;;
    esac
    grep -q "$BIN" "$RC" 2>/dev/null || echo "export PATH=\"$BIN:\$PATH\"" >> "$RC"
    export PATH="$BIN:$PATH"
    echo "  ✓ Added to $RC"
else echo "  ✓ Already in PATH"; fi

echo "[3/4] Ollama..."
if command -v ollama >/dev/null 2>&1; then echo "  ✓ Already installed"
else
    read -p "  ⚠ Install Ollama for AI? [Y/n] " R; R=${R:-Y}
    if [[ "$R" =~ ^[Yy]$ ]]; then
        curl -fsSL https://ollama.ai/install.sh | sh
        command -v ollama >/dev/null 2>&1 && { echo "ollama" >> "$MANIFEST"; echo "  ✓ Installed"; } || echo "  ⚠ Failed"
    else echo "  - Skipped"; fi
fi

echo "[4/4] AI Model..."
if command -v ollama >/dev/null 2>&1; then
    read -p "  Download qwen3:8b (~4.9GB)? [Y/n] " R; R=${R:-Y}
    if [[ "$R" =~ ^[Yy]$ ]]; then
        ollama serve >/dev/null 2>&1 & sleep 3
        ollama pull qwen3:8b && echo "  ✓ Ready" || echo "  ⚠ Failed"
        kill %1 2>/dev/null || true
    fi
fi

echo ""
echo "══════════════════════════════════════════════"
echo "  ✅ OneBrain installed!"
echo "  Run: onebrain-dashboard"
echo "  Open: http://localhost:4280"
echo "══════════════════════════════════════════════"
'@
    Set-Content -Path "$PkgDir\install.sh" -Value $InstallContent -Encoding UTF8

    $UninstallContent = @'
#!/usr/bin/env bash
set -euo pipefail
echo "🧠 OneBrain Uninstaller"
read -p "Remove OneBrain? [Y/n] " C; C=${C:-Y}
[[ ! "$C" =~ ^[Yy]$ ]] && echo "Cancelled." && exit 0
if [ "$(id -u)" -eq 0 ]; then P="/usr/local"; else P="$HOME/.local"; fi
M="$P/share/onebrain/.installed-by-onebrain"
TOOLS=""; [ -f "$M" ] && TOOLS=$(cat "$M" | grep -v '^$')
rm -f "$P/bin/onebrain" "$P/bin/onebrain-dashboard"
rm -rf "$P/share/onebrain"
echo "  ✓ OneBrain removed"
if echo "$TOOLS" | grep -q "^ollama$"; then
    read -p "  Remove Ollama (installed by OneBrain)? [y/N] " R
    if [[ "${R:-N}" =~ ^[Yy]$ ]]; then
        ollama rm qwen3:8b 2>/dev/null || true
        sudo rm -f /usr/local/bin/ollama 2>/dev/null
        sudo rm -rf /usr/local/lib/ollama 2>/dev/null
        echo "  ✓ Ollama removed"
    fi
fi
echo "Done."
'@
    Set-Content -Path "$PkgDir\uninstall.sh" -Value $UninstallContent -Encoding UTF8
}

# ── Package Windows (always works) ───────────────────────────
Write-Host '  Packaging Windows...' -ForegroundColor White
$WinArchive = New-ReleasePackage `
    -PkgName 'onebrain-windows-x64' `
    -BinarySource "$SrcDir\target\release\onebrain.exe" `
    -BinaryDest 'onebrain.exe' `
    -IsWindows $true
$WinSize = '{0:N1} MB' -f ((Get-Item $WinArchive).Length / 1MB)
Write-Host "  OK onebrain-windows-x64.zip ($WinSize)" -ForegroundColor Green

# ── Package Linux (if cross-compiled) ────────────────────────
$LinuxArchive = $null
$LinuxBin = "$SrcDir\target\x86_64-unknown-linux-gnu\release\onebrain"
if (Test-Path $LinuxBin) {
    Write-Host '  Packaging Linux...' -ForegroundColor White
    $LinuxArchive = New-ReleasePackage `
        -PkgName 'onebrain-linux-x86_64' `
        -BinarySource $LinuxBin `
        -BinaryDest 'onebrain' `
        -IsWindows $false
    $LinuxSize = '{0:N1} MB' -f ((Get-Item $LinuxArchive).Length / 1MB)
    Write-Host "  OK onebrain-linux-x86_64 ($LinuxSize)" -ForegroundColor Green
} else {
    Write-Host '  ! Linux: skipped cross-compile not available' -ForegroundColor Yellow
    Write-Host '    → Build on Linux with: bash installer/release.sh' -ForegroundColor DarkGray
    
    # Create package without binary (web + scripts only)
    Write-Host "  Creating Linux package web + scripts only, needs binary..." -ForegroundColor DarkGray
    $PkgDir = Join-Path $ReleaseDir 'onebrain-linux-x86_64'
    New-Item -Path "$PkgDir\bin" -ItemType Directory -Force | Out-Null
    New-Item -Path "$PkgDir\web" -ItemType Directory -Force | Out-Null
    Copy-Item "$WebDir\dist\*" "$PkgDir\web\" -Recurse -Force
    Create-UnixInstallerScripts -PkgDir $PkgDir
    '[!] Binary not included. Build on Linux: cargo build --release -p onebrain-cli' | Set-Content "$PkgDir\bin\BUILD_ON_LINUX.txt"
    @"
OneBrain - Linux Package (web + scripts)
NOTE: Binary needs to be built on Linux.
Run: cargo build --release -p onebrain-cli
Then copy target/release/onebrain to bin/
"@ | Set-Content "$PkgDir\README.txt" -Encoding UTF8
    try {
        Push-Location $ReleaseDir
        tar -czf 'onebrain-linux-x86_64.tar.gz' 'onebrain-linux-x86_64'
        Pop-Location
    } catch {
        Compress-Archive -Path $PkgDir -DestinationPath "$ReleaseDir\onebrain-linux-x86_64.zip"
    }
}

# ── Package macOS (if cross-compiled) ────────────────────────
$MacArchive = $null
$MacBin = "$SrcDir\target\aarch64-apple-darwin\release\onebrain"
if (Test-Path $MacBin) {
    Write-Host '  Packaging macOS...' -ForegroundColor White
    $MacArchive = New-ReleasePackage `
        -PkgName 'onebrain-macos-arm64' `
        -BinarySource $MacBin `
        -BinaryDest 'onebrain' `
        -IsWindows $false
    $MacSize = '{0:N1} MB' -f ((Get-Item $MacArchive).Length / 1MB)
    Write-Host "  OK onebrain-macos-arm64 ($MacSize)" -ForegroundColor Green
} else {
    Write-Host '  ! macOS: skipped cross-compile not available' -ForegroundColor Yellow
    Write-Host '    → Build on macOS with: bash installer/release.sh' -ForegroundColor DarkGray
    
    $PkgDir = Join-Path $ReleaseDir 'onebrain-macos-arm64'
    New-Item -Path "$PkgDir\bin" -ItemType Directory -Force | Out-Null
    New-Item -Path "$PkgDir\web" -ItemType Directory -Force | Out-Null
    Copy-Item "$WebDir\dist\*" "$PkgDir\web\" -Recurse -Force
    Create-UnixInstallerScripts -PkgDir $PkgDir
    '[!] Binary not included. Build on macOS: cargo build --release -p onebrain-cli' | Set-Content "$PkgDir\bin\BUILD_ON_MACOS.txt"
    @"
OneBrain - macOS Package (web + scripts)
NOTE: Binary needs to be built on macOS.
Run: cargo build --release -p onebrain-cli
Then copy target/release/onebrain to bin/
"@ | Set-Content "$PkgDir\README.txt" -Encoding UTF8
    try {
        Push-Location $ReleaseDir
        tar -czf 'onebrain-macos-arm64.tar.gz' 'onebrain-macos-arm64'
        Pop-Location
    } catch {
        Compress-Archive -Path $PkgDir -DestinationPath "$ReleaseDir\onebrain-macos-arm64.zip"
    }
}
Write-Host ''

# ══════════════════════════════════════════════════════════════
# Step 5: Summary
# ══════════════════════════════════════════════════════════════
Write-Host '[5/5] Summary' -ForegroundColor Yellow
Write-Host ''
Write-Host '================================================' -ForegroundColor Cyan
Write-Host '  Release packages created!' -ForegroundColor Green
Write-Host ''

$Packages = @(Get-ChildItem $ReleaseDir -Filter '*.zip' -File)
$Packages += @(Get-ChildItem $ReleaseDir -Filter '*.tar.gz' -File)

foreach ($Pkg in $Packages) {
    $Size = '{0:N1} MB' -f ($Pkg.Length / 1MB)
    $Status = if ($Pkg.Name -like '*windows*') { '[OK]' } 
              elseif ((Get-Content "$ReleaseDir\$($Pkg.BaseName -replace '\.tar$','')\bin\*" -ErrorAction SilentlyContinue) -match 'BUILD_ON') { '[!] web only' }
              else { '[OK]' }
    Write-Host "  $Status $($Pkg.Name) ($Size)" -ForegroundColor White
}

Write-Host ''
Write-Host "  Path: $ReleaseDir" -ForegroundColor DarkGray
Write-Host ''
Write-Host '  Gửi file tương ứng cho user:' -ForegroundColor White
Write-Host '    Windows: .zip → giải nén → click install.ps1' -ForegroundColor Yellow
Write-Host '    Linux:   .tar.gz → tar xzf → ./install.sh' -ForegroundColor Yellow
Write-Host '    macOS:   .tar.gz → tar xzf → ./install.sh' -ForegroundColor Yellow
Write-Host '================================================' -ForegroundColor Cyan
