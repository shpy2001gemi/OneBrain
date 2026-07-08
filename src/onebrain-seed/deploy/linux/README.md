# OneBrain Seed — Linux Deployment
# systemd service + Docker cho production.

## Quick Start (Console)

```bash
# Build
cargo build --release -p onebrain-seed

# Run
./target/release/onebrain-seed --port 4242 --name "Seed-VN1" --max-peers 10000
```

## systemd Service (Production)

### 1. Copy binary

```bash
sudo cp target/release/onebrain-seed /usr/local/bin/
sudo chmod +x /usr/local/bin/onebrain-seed
```

### 2. Create systemd unit

```bash
sudo cp deploy/linux/onebrain-seed.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable onebrain-seed
sudo systemctl start onebrain-seed
```

### 3. Quản lý

```bash
sudo systemctl status onebrain-seed    # Status
sudo systemctl restart onebrain-seed   # Restart
sudo journalctl -u onebrain-seed -f    # Log real-time
```

## Docker

```bash
# Build image
docker build -t onebrain-seed -f deploy/linux/Dockerfile .

# Run
docker run -d \
  --name onebrain-seed \
  --restart unless-stopped \
  -p 4242:4242 \
  onebrain-seed \
  --port 4242 --name "Seed-VN1" --max-peers 10000
```

## Firewall (UFW)

```bash
sudo ufw allow 4242/tcp comment "OneBrain Seed"
```

## Cross-compile từ Windows

```bash
# Cài target
rustup target add x86_64-unknown-linux-gnu

# Build
cargo build --release -p onebrain-seed --target x86_64-unknown-linux-gnu

# Copy lên server
scp target/x86_64-unknown-linux-gnu/release/onebrain-seed user@server:/usr/local/bin/
```
