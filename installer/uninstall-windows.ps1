# OneBrain Uninstaller — Windows
# Removes binary, web dashboard, and optionally data + AI models

$ErrorActionPreference = 'Stop'

Write-Host ''
Write-Host '  +================================================+' -ForegroundColor Cyan
Write-Host '  |     OneBrain Uninstaller — Windows             |' -ForegroundColor Cyan
Write-Host '  +================================================+' -ForegroundColor Cyan
Write-Host ''

$InstallDir = Join-Path $env:LOCALAPPDATA 'OneBrain'
$InstallBin = Join-Path $InstallDir 'bin'
$InstallWeb = Join-Path $InstallDir 'web'

# ── Detect what's installed ──────────────────────────────────
Write-Host 'Scanning installation...' -ForegroundColor Yellow
Write-Host ''

$FoundSomething = $false

if (Test-Path "$InstallBin\onebrain.exe") {
    $BinSize = '{0:N1} MB' -f ((Get-Item "$InstallBin\onebrain.exe").Length / 1MB)
    Write-Host "  [x] Binary:    $InstallBin\onebrain.exe ($BinSize)" -ForegroundColor White
    $FoundSomething = $true
} else {
    Write-Host '  [ ] Binary:    not found' -ForegroundColor DarkGray
}

if (Test-Path "$InstallDir\OneBrain Dashboard.bat") {
    Write-Host "  [x] Launcher:  $InstallDir\OneBrain Dashboard.bat" -ForegroundColor White
    $FoundSomething = $true
} else {
    Write-Host '  [ ] Launcher:  not found' -ForegroundColor DarkGray
}

if (Test-Path $InstallWeb) {
    $WebCount = (Get-ChildItem $InstallWeb -Recurse -File -ErrorAction SilentlyContinue).Count
    Write-Host "  [x] Web:       $InstallWeb ($WebCount files)" -ForegroundColor White
    $FoundSomething = $true
} else {
    Write-Host '  [ ] Web:       not found' -ForegroundColor DarkGray
}

# Check data directories
$DataDirs = @()
$DataCandidates = @(
    '.\onebrain_data',
    (Join-Path $env:USERPROFILE 'onebrain_data'),
    (Join-Path $env:USERPROFILE '.onebrain'),
    (Join-Path $env:APPDATA 'OneBrain')
)
foreach ($Dir in $DataCandidates) {
    if (Test-Path $Dir) {
        $DirSize = '{0:N1} MB' -f ((Get-ChildItem $Dir -Recurse -File -ErrorAction SilentlyContinue | Measure-Object -Property Length -Sum).Sum / 1MB)
        Write-Host "  [x] Data:      $Dir ($DirSize)" -ForegroundColor White
        $DataDirs += $Dir
    }
}

# Check build directory
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectRoot = Split-Path -Parent $ScriptDir
$BuildDir = Join-Path $ProjectRoot 'build'
if (Test-Path $BuildDir) {
    $BuildSize = '{0:N1} MB' -f ((Get-ChildItem $BuildDir -Recurse -File -ErrorAction SilentlyContinue | Measure-Object -Property Length -Sum).Sum / 1MB)
    Write-Host "  [x] Build:     $BuildDir ($BuildSize)" -ForegroundColor White
}

# Check Ollama
$HasOllama = [bool](Get-Command ollama -ErrorAction SilentlyContinue)
if ($HasOllama) {
    Write-Host '  [x] Ollama:    installed' -ForegroundColor White
}

Write-Host ''

if (-not $FoundSomething) {
    Write-Host "OneBrain is not installed at $InstallDir." -ForegroundColor Yellow
    Write-Host 'Nothing to remove.'
    exit 0
}

# ── Confirm ──────────────────────────────────────────────────
Write-Host '------------------------------------------------' -ForegroundColor DarkGray
$Confirm = Read-Host 'Remove OneBrain binaries and web dashboard? [Y/n]'
if ($Confirm -and $Confirm -notmatch '^[Yy]') {
    Write-Host 'Cancelled.'
    exit 0
}

# ── Remove core installation ─────────────────────────────────
Write-Host ''
Write-Host '[1/5] Removing binaries & web...' -ForegroundColor Yellow

if (Test-Path $InstallDir) {
    Remove-Item $InstallDir -Recurse -Force
    Write-Host "  OK Removed $InstallDir" -ForegroundColor Green
} else {
    Write-Host '  - Nothing to remove'
}

# ── Remove from PATH ─────────────────────────────────────────
Write-Host '[2/5] Cleaning PATH...' -ForegroundColor Yellow
$CurrentPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if ($CurrentPath -like "*$InstallBin*") {
    $NewPath = ($CurrentPath -split ';' | Where-Object { $_ -ne $InstallBin -and $_ -ne '' }) -join ';'
    [Environment]::SetEnvironmentVariable('Path', $NewPath, 'User')
    Write-Host '  OK Removed from PATH' -ForegroundColor Green
    Write-Host '  Note: Restart terminal for PATH changes to take effect' -ForegroundColor DarkGray
} else {
    Write-Host '  - Not in PATH'
}

# ── Optional: remove build directory ─────────────────────────
Write-Host '[3/5] Build artifacts...' -ForegroundColor Yellow
if (Test-Path $BuildDir) {
    $ConfirmBuild = Read-Host "  Remove build directory ($BuildDir)? [Y/n]"
    if (-not $ConfirmBuild -or $ConfirmBuild -match '^[Yy]') {
        Remove-Item $BuildDir -Recurse -Force
        Write-Host '  OK Removed build directory' -ForegroundColor Green
    } else {
        Write-Host '  - Kept'
    }
} else {
    Write-Host '  - No build directory'
}

# ── Optional: remove data ────────────────────────────────────
Write-Host '[4/5] Knowledge data...' -ForegroundColor Yellow
if ($DataDirs.Count -gt 0) {
    Write-Host ''
    Write-Host '  WARNING: This will permanently delete your knowledge data!' -ForegroundColor Red
    Write-Host '  Data directories found:' -ForegroundColor Red
    foreach ($Dir in $DataDirs) {
        Write-Host "    - $Dir" -ForegroundColor Red
    }
    Write-Host ''
    $ConfirmData = Read-Host '  Delete ALL knowledge data? [y/N]'
    if ($ConfirmData -match '^[Yy]') {
        foreach ($Dir in $DataDirs) {
            Remove-Item $Dir -Recurse -Force
            Write-Host "  OK Removed $Dir" -ForegroundColor Green
        }
    } else {
        Write-Host '  - Data preserved'
    }
} else {
    Write-Host '  - No data directories found'
}

# ── Optional: remove prerequisites installed by OneBrain ──────
Write-Host '[5/5] Prerequisites...' -ForegroundColor Yellow

$Manifest = Join-Path $InstallDir '.installed-by-onebrain'
if (-not (Test-Path $Manifest)) {
    $Manifest = Join-Path $env:LOCALAPPDATA 'OneBrain\.installed-by-onebrain'
}

if (Test-Path $Manifest) {
    $InstalledTools = Get-Content $Manifest | Where-Object { $_ -ne '' }
    if ($InstalledTools.Count -gt 0) {
        Write-Host ''
        Write-Host '  The following were installed BY OneBrain auto-installer.' -ForegroundColor White
        Write-Host '  Each will be asked individually:' -ForegroundColor White
        Write-Host ''

        $HasWinget = $false
        try { winget --version | Out-Null; $HasWinget = $true } catch {}

        # Ask for each tool one by one
        if ($InstalledTools -contains 'rust') {
            $ConfirmRust = Read-Host '  Remove Rust (rustup)? [y/N]'
            if ($ConfirmRust -match '^[Yy]') {
                if (Get-Command rustup -ErrorAction SilentlyContinue) {
                    try {
                        rustup self uninstall -y 2>$null
                        Write-Host '  OK Rust uninstalled' -ForegroundColor Green
                    } catch {
                        Write-Host '  - Rust: manual removal needed'
                    }
                }
            } else {
                Write-Host '  - Rust: kept'
            }
        }

        if ($InstalledTools -contains 'nodejs') {
            $ConfirmNode = Read-Host '  Remove Node.js? [y/N]'
            if ($ConfirmNode -match '^[Yy]') {
                if ($HasWinget) {
                    winget uninstall --id OpenJS.NodeJS.LTS -e --silent 2>$null
                    Write-Host '  OK Node.js uninstalled' -ForegroundColor Green
                } else {
                    Write-Host '  - Node.js: uninstall from Control Panel' -ForegroundColor Yellow
                }
            } else {
                Write-Host '  - Node.js: kept'
            }
        }

        if ($InstalledTools -contains 'ollama') {
            $ConfirmOllama = Read-Host '  Remove Ollama + AI models? [y/N]'
            if ($ConfirmOllama -match '^[Yy]') {
                try { ollama rm qwen3:8b 2>$null } catch {}
                if ($HasWinget) {
                    winget uninstall --id Ollama.Ollama -e --silent 2>$null
                    Write-Host '  OK Ollama uninstalled' -ForegroundColor Green
                } else {
                    Write-Host '  - Ollama: uninstall from Control Panel' -ForegroundColor Yellow
                }
            } else {
                Write-Host '  - Ollama: kept'
            }
        }

        if ($InstalledTools -contains 'git') {
            Write-Host '  - Git: keeping (commonly needed by other tools)'
        }

        # Remove manifest
        Remove-Item $Manifest -Force -ErrorAction SilentlyContinue
    }
} else {
    # No manifest — prerequisites were NOT installed by OneBrain
    Write-Host ''
    Write-Host '  No installer manifest found.' -ForegroundColor DarkGray
    Write-Host '  Prerequisites were not installed by OneBrain — keeping them.' -ForegroundColor DarkGray

    if ($HasOllama) {
        $ConfirmModel = Read-Host '  Remove OneBrain AI model (qwen3:8b) only? [y/N]'
        if ($ConfirmModel -match '^[Yy]') {
            try {
                ollama rm qwen3:8b 2>$null
                Write-Host '  OK Removed qwen3:8b model' -ForegroundColor Green
            } catch {
                Write-Host '  - Model not found'
            }
        } else {
            Write-Host '  - AI model preserved'
        }
    }
}

# ── Summary ──────────────────────────────────────────────────
Write-Host ''
Write-Host '================================================' -ForegroundColor Cyan
Write-Host 'OneBrain uninstalled.' -ForegroundColor Green
Write-Host '================================================' -ForegroundColor Cyan
