# OneBrain Seed — Windows Deployment
# Chạy seed node như Windows Service hoặc console app.

## Quick Start (Console)

```powershell
# Build
cargo build --release -p onebrain-seed

# Run
.\target\release\onebrain-seed.exe --port 4242 --name "Seed-VN1" --max-peers 10000
```

## Windows Service (Production)

### Cài đặt service với NSSM

```powershell
# 1. Tải NSSM: https://nssm.cc/download
# 2. Cài service
nssm install OneBrainSeed "C:\OneBrain\onebrain-seed.exe"
nssm set OneBrainSeed AppParameters "--port 4242 --name Seed-VN1 --max-peers 10000"
nssm set OneBrainSeed AppDirectory "C:\OneBrain"
nssm set OneBrainSeed DisplayName "OneBrain Seed Node"
nssm set OneBrainSeed Description "OneBrain P2P relay and peer discovery"
nssm set OneBrainSeed Start SERVICE_AUTO_START
nssm set OneBrainSeed AppStdout "C:\OneBrain\logs\seed.log"
nssm set OneBrainSeed AppStderr "C:\OneBrain\logs\seed-error.log"
nssm set OneBrainSeed AppRotateFiles 1
nssm set OneBrainSeed AppRotateBytes 10485760

# 3. Start service
nssm start OneBrainSeed
```

### Quản lý service

```powershell
nssm status OneBrainSeed     # Check status
nssm stop OneBrainSeed       # Stop
nssm restart OneBrainSeed    # Restart
nssm remove OneBrainSeed     # Uninstall
```

## Firewall Rule

```powershell
New-NetFirewallRule -DisplayName "OneBrain Seed" `
    -Direction Inbound -Protocol TCP -LocalPort 4242 `
    -Action Allow -Profile Domain,Private,Public
```

## Monitoring

```powershell
# Xem log real-time
Get-Content "C:\OneBrain\logs\seed.log" -Tail 50 -Wait
```
