# onebrain-seed — legacy compatibility only

Legacy TCP/JSON peer discovery and relay server. It is **not** the vNext
`onebrain-relay` and supplies no signed vNext bootstrap, identity or route
authority. Keep it only for an explicitly approved rollback/compatibility
window. The daemon refuses to start without `--legacy-seed-compat`.

## Kiến trúc

Seed node là **server infrastructure** — chạy trên VPS/server, KHÔNG phải trên máy user.
Code Rust duy nhất, compile ra binary cho từng platform.

```
onebrain-seed/
├── src/                    # Rust source (cross-platform)
│   ├── main.rs             # Entry point + CLI
│   ├── server.rs           # TCP listener + connection handler
│   ├── registry.rs         # Peer registry (register, heartbeat, cleanup)
│   └── relay.rs            # Message relay between peers
│
├── deploy/                 # Deploy scripts & configs theo platform
│   ├── windows/            # Windows Service + PowerShell
│   ├── linux/              # systemd + Docker + shell scripts
│   └── arduino/            # Embedded seed (future — ESP32/RPi)
│
└── Cargo.toml
```

## Chức năng

| Feature | Mô tả |
|---------|-------|
| **Peer Registration** | Node đăng ký với seed, nhận peer list |
| **Heartbeat** | Mỗi 60s, stale cleanup sau 300s |
| **Peer Discovery** | Trả danh sách peers đang online |
| **Message Relay** | Relay messages giữa peers sau NAT |
| **Stats Logging** | Log peer count, relay count mỗi 30s |

## Build & Run

```bash
# Build cho platform hiện tại
cargo build --release -p onebrain-seed

# Chạy
onebrain-seed --legacy-seed-compat --port 4242 --name "Seed-N1" --max-peers 10000
```

## Cross-compile

```bash
# Windows → Linux
cargo build --release -p onebrain-seed --target x86_64-unknown-linux-gnu

# Windows → ARM (Raspberry Pi)
cargo build --release -p onebrain-seed --target aarch64-unknown-linux-gnu
```

## Platforms

| Platform | Status | Deploy method |
|----------|--------|--------------|
| **Windows** | ✅ Ready | PowerShell + Windows Service |
| **Linux** | ✅ Ready | systemd + Docker |
| **macOS** | ✅ Ready | launchd |
| **Raspberry Pi** | 🟡 Planned | Cross-compile ARM + systemd |
| **Arduino/ESP32** | 🟡 Planned | Embedded Rust (no_std) hoặc C port |
