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
