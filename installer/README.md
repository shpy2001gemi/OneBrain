# 🧠 OneBrain — Hướng dẫn Cài đặt & Cấu hình

> **OneBrain** — Decentralized Knowledge Network
> CLI + Web Dashboard + AI Engine

---

## ⚡ Auto Install (1 lệnh duy nhất)

Script tự động cài đặt mọi thứ: Git, Rust, Node.js, Ollama, build, install, tải AI model.

### Linux / macOS

```bash
curl -fsSL https://raw.githubusercontent.com/<your-org>/OneBrain/main/installer/auto-install.sh | bash
```

Hoặc chạy local:
```bash
chmod +x installer/auto-install.sh
./installer/auto-install.sh
```

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/<your-org>/OneBrain/main/installer/auto-install.ps1 | iex
```

Hoặc chạy local:
```powershell
.\installer\auto-install.ps1
```

### Auto-install sẽ thực hiện:

| Bước | Hành động | Chi tiết |
|------|-----------|----------|
| 1 | **Cài prerequisites** | Git, Rust (rustup), Node.js, Ollama — chỉ cài phần chưa có |
| 2 | **Clone repository** | `git clone --depth 1` vào thư mục tạm |
| 3 | **Build CLI** | `cargo build --release` (Rust binary) |
| 4 | **Build Web** | `npm install && npm run build` (React/Vite) |
| 5 | **Install** | Copy binary + web vào vị trí chuẩn |
| 6 | **Tải AI model** | `ollama pull qwen3:8b` (mặc định) |
| 7 | **Cấu hình PATH** | Tự thêm vào PATH (bash/zsh/fish hoặc Windows) |

Sau khi xong → chạy `onebrain-dashboard` → mở `http://localhost:4280`

> **Không muốn auto-install?** Xem hướng dẫn cài thủ công bên dưới.



## Mục lục

1. [Yêu cầu hệ thống](#1-yêu-cầu-hệ-thống)
2. [Cài đặt Prerequisites](#2-cài-đặt-prerequisites)
3. [Build OneBrain](#3-build-onebrain)
4. [Cài đặt (Install)](#4-cài-đặt-install)
5. [Cấu hình AI Model](#5-cấu-hình-ai-model)
6. [Chạy OneBrain](#6-chạy-onebrain)
7. [Cấu hình nâng cao](#7-cấu-hình-nâng-cao)
8. [Kiến trúc hệ thống](#8-kiến-trúc-hệ-thống)
9. [Gỡ cài đặt](#9-gỡ-cài-đặt)
10. [Xử lý lỗi thường gặp](#10-xử-lý-lỗi-thường-gặp)

---

## 1. Yêu cầu hệ thống

### Bắt buộc

| Phần mềm | Phiên bản | Link tải | Ghi chú |
|-----------|-----------|----------|---------|
| **Rust** | 1.75+ | https://rustup.rs | Compiler cho backend |
| **Node.js** | 18+ | https://nodejs.org | Build web dashboard |

### Khuyến nghị (cho AI features)

| Phần mềm | Phiên bản | Link tải | Ghi chú |
|-----------|-----------|----------|---------|
| **Ollama** | 0.2+ | https://ollama.ai | Local LLM runtime |
| **Git** | 2.30+ | https://git-scm.com | Quản lý source |

### Yêu cầu phần cứng

| Thành phần | Tối thiểu | Khuyến nghị |
|------------|-----------|-------------|
| RAM | 4 GB | 16 GB (cho AI model lớn) |
| Disk | 2 GB | 20 GB (model AI + knowledge data) |
| GPU | Không bắt buộc | NVIDIA 6GB+ VRAM (tăng tốc AI) |

---

## 2. Cài đặt Prerequisites

### 2.1. Cài Rust

**Windows** (PowerShell):
```powershell
# Tải và chạy rustup
Invoke-WebRequest -Uri "https://win.rustup.rs" -OutFile rustup-init.exe
.\rustup-init.exe -y
# Khởi động lại terminal sau khi cài
```

**Linux**:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source $HOME/.cargo/env
```

**macOS**:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source $HOME/.cargo/env
```

Kiểm tra:
```bash
rustc --version    # Kỳ vọng: rustc 1.75.0 trở lên
cargo --version
```

### 2.2. Cài Node.js

**Windows**: Tải installer từ https://nodejs.org (chọn LTS)

**Linux** (Ubuntu/Debian):
```bash
curl -fsSL https://deb.nodesource.com/setup_20.x | sudo -E bash -
sudo apt-get install -y nodejs
```

**macOS** (Homebrew):
```bash
brew install node
```

Kiểm tra:
```bash
node --version     # Kỳ vọng: v18.0.0 trở lên
npm --version
```

### 2.3. Cài Ollama (AI Engine)

> ⚠️ **Ollama không bắt buộc** nhưng cần thiết cho: Chat, Encode (phân loại gene type), và tất cả tính năng AI.

**Windows**: Tải installer từ https://ollama.ai/download

**Linux**:
```bash
curl -fsSL https://ollama.ai/install.sh | sh
```

**macOS** (Homebrew):
```bash
brew install ollama
```

Kiểm tra:
```bash
ollama --version
```

---

## 3. Build OneBrain

### 3.1. Clone repository

```bash
git clone https://github.com/<your-org>/OneBrain.git
cd OneBrain
```

### 3.2. Build tự động (khuyến nghị)

**Windows**:
```powershell
cd installer
.\build.ps1
```

**Linux / macOS**:
```bash
cd installer
chmod +x *.sh
./build.sh
```

Script sẽ tự động:
1. ✅ Kiểm tra Rust và Node.js
2. ✅ Build CLI binary (release mode)
3. ✅ Build Web Dashboard (Vite production)
4. ✅ Tạo distribution package trong `build/`
5. ✅ Tạo launcher script

### 3.3. Build thủ công (nếu cần)

```bash
cd src

# Build Rust CLI
cargo build --release -p onebrain-cli

# Build Web Dashboard
cd onebrain-web
npm install
npm run build
```

### 3.4. Kết quả build

```
build/
├── bin/
│   └── onebrain(.exe)     # CLI + API server binary
├── web/                    # Web Dashboard (static files)
│   ├── index.html
│   └── assets/
│       ├── index-*.css     # ~7.5 KB (design system)
│       └── index-*.js      # ~293 KB (React app)
└── start.sh / start.bat    # Quick launcher
```

---

## 4. Cài đặt (Install)

### Windows

```powershell
cd installer
.\install-windows.ps1
```

- Cài vào: `%LOCALAPPDATA%\OneBrain\`
- Tự động thêm vào PATH
- Tạo launcher: `OneBrain Dashboard.bat`

### Linux

```bash
# User install (không cần sudo)
./install-linux.sh

# System-wide install
sudo ./install-linux.sh
```

| Mode | Vị trí binary | Vị trí web | Launcher |
|------|--------------|------------|----------|
| User | `~/.local/bin/` | `~/.local/share/onebrain/web/` | `onebrain-dashboard` |
| System | `/usr/local/bin/` | `/usr/local/share/onebrain/web/` | `onebrain-dashboard` |

### macOS

```bash
# User install
./install-macos.sh

# System-wide install
sudo ./install-macos.sh
```

Giống Linux, kèm tự động xóa quarantine attribute (`xattr -cr`).

### Thêm vào PATH (nếu cần)

Nếu terminal báo `onebrain: command not found`:

```bash
# Thêm vào ~/.bashrc hoặc ~/.zshrc:
export PATH="$HOME/.local/bin:$PATH"

# Reload:
source ~/.bashrc   # hoặc source ~/.zshrc
```

### Những gì được cài

| Component | Mô tả |
|-----------|--------|
| `onebrain` | Binary chính — CLI REPL + API server tích hợp |
| `onebrain-dashboard` | Script khởi động nhanh (CLI + API + Web) |
| `web/` | Web Dashboard đã build (static HTML/CSS/JS) |

---

## 5. Cấu hình AI Model

### 5.1. Khởi động Ollama

```bash
ollama serve
```

> Ollama chạy ở `http://localhost:11434`. Giữ terminal này mở.

### 5.2. Chọn và tải model

OneBrain hoạt động với bất kỳ model Ollama nào. Khuyến nghị:

| Model | VRAM cần | Tốc độ | Chất lượng | Lệnh tải |
|-------|----------|--------|------------|----------|
| `qwen3:1.7b` | 2 GB | ⚡⚡⚡ Rất nhanh | ★★☆☆ | `ollama pull qwen3:1.7b` |
| **`qwen3:8b`** ⭐ | 6 GB | ⚡⚡ Nhanh | ★★★☆ | `ollama pull qwen3:8b` |
| `llama3.2:3b` | 3 GB | ⚡⚡⚡ Nhanh | ★★★☆ | `ollama pull llama3.2` |
| `llama3.1:8b` | 6 GB | ⚡⚡ Nhanh | ★★★☆ | `ollama pull llama3.1:8b` |
| `gemma3:12b` | 8 GB | ⚡ Trung bình | ★★★★ | `ollama pull gemma3:12b` |
| `qwen3:32b` | 20 GB | 🐢 Chậm | ★★★★★ | `ollama pull qwen3:32b` |

**Khuyến nghị mặc định**: `qwen3:8b` — cân bằng tốt giữa tốc độ và chất lượng.

```bash
# Tải model khuyến nghị
ollama pull qwen3:8b

# Kiểm tra model đã cài
ollama list

# Test thử
ollama run qwen3:8b "Hello, what is photosynthesis?"
```

### 5.3. Chỉ định model khi khởi động

```bash
# Dùng model mặc định (qwen3:8b)
onebrain start --api

# Dùng model khác
onebrain start --api --model llama3.2

# Dùng model lớn cho chất lượng cao
onebrain start --api --model qwen3:32b
```

### 5.4. Đổi model trong lúc chạy

**Qua CLI REPL:**
```
> /model llama3.2
```

**Qua Web Dashboard:**
1. Mở http://localhost:4280
2. Vào tab **Dashboard** → AI Status card → xem model hiện tại
3. Dùng API: `POST /api/ai/model` với body `{"model_name": "llama3.2"}`

**Qua REST API:**
```bash
# Xem danh sách models
curl -H "Authorization: Bearer onebrain-dev-token" \
  http://localhost:4280/api/ai/models

# Chuyển model
curl -X POST http://localhost:4280/api/ai/model \
  -H "Authorization: Bearer onebrain-dev-token" \
  -H "Content-Type: application/json" \
  -d '{"model_name": "llama3.2"}'

# Kiểm tra kết nối AI
curl -H "Authorization: Bearer onebrain-dev-token" \
  http://localhost:4280/api/ai/status
```

### 5.5. Dùng Ollama remote (máy khác)

Nếu Ollama chạy trên máy khác (ví dụ: GPU server):

```bash
onebrain start --api --ollama-url "http://192.168.1.100:11434"
```

### 5.6. Chạy không có AI

OneBrain vẫn hoạt động mà không cần Ollama — chỉ mất tính năng:
- ❌ Chat (Mediator)
- ❌ Auto gene-type classification khi Encode
- ✅ Manual encode (text → KU)
- ✅ Search / KQL
- ✅ P2P networking
- ✅ Knowledge graph
- ✅ Wallet / PoMV

---

## 6. Chạy OneBrain

### 6.1. Quick Start (1 lệnh)

```bash
onebrain-dashboard
```

Hoặc:
```bash
onebrain start --api
```

Mở trình duyệt: **http://localhost:4280**
Nhập token: `onebrain-dev-token`

### 6.2. Chạy đầy đủ (step-by-step)

```bash
# Terminal 1: Ollama (nếu dùng AI)
ollama serve

# Terminal 2: OneBrain
onebrain start \
  --name "MyBrain" \
  --api \
  --api-token "my-secure-token-here" \
  --model qwen3:8b
```

### 6.3. Tất cả CLI flags

```bash
onebrain start [OPTIONS]
```

| Flag | Mặc định | Mô tả |
|------|----------|-------|
| `--name` | `OneBrain` | Tên node (hiển thị cho peers) |
| `--port` | `4242` | Port P2P networking |
| `--data-dir` | `./onebrain_data` | Thư mục lưu dữ liệu |
| `--ollama-url` | `http://localhost:11434` | Ollama API endpoint |
| `--model` | `qwen3:8b` | Tên model AI |
| `--seeds` | — | Seed nodes, comma-separated |
| `--api` | `false` | Bật API server + Web Dashboard |
| `--api-port` | `4280` | Port API server |
| `--api-token` | `onebrain-dev-token` | Token xác thực |
| `--web-dir` | auto-detect | Đường dẫn tới web dashboard đã build |

### 6.4. Các lệnh REPL

Sau khi khởi động, gõ lệnh trong terminal:

| Lệnh | Mô tả | Ví dụ |
|-------|--------|-------|
| `encode "text"` | Encode văn bản thành KU | `encode "Nước sôi ở 100°C"` |
| `search query` | Tìm kiếm knowledge | `search quantum computing` |
| `kql QUERY` | Chạy KQL query | `kql FIND WHERE gene_type = "Fact"` |
| `status` | Xem trạng thái node | `status` |
| `peers` | Danh sách peers P2P | `peers` |
| `connect IP:PORT` | Kết nối peer mới | `connect 192.168.1.10:4242` |
| `help` | Xem tất cả lệnh | `help` |
| Free text | Chat với AI | `What is machine learning?` |

### 6.5. Web Dashboard screens

| # | Screen | URL | Tính năng |
|---|--------|-----|-----------|
| 1 | Dashboard | `/` | Stats tổng quan, AI status, recent KUs |
| 2 | Explorer | `/explorer` | Browse, search, filter KUs |
| 3 | Encode | `/encode` | Encode text → Knowledge Unit |
| 4 | Chat | `/chat` | Chat AI interface |
| 5 | Graph | `/graph` | Interactive knowledge graph (SVG) |
| 6 | PoMV | `/pomv` | PoMV 6-dimension analysis |
| 7 | Network | `/network` | P2P peer management |
| 8 | Wallet | `/wallet` | OBT balance & transactions |

---

## 7. Cấu hình nâng cao

### 7.1. Thay đổi data directory

```bash
onebrain start --data-dir ~/my-knowledge-base
```

Dữ liệu lưu tại đây:
```
onebrain_data/
├── ku_store.redb      # Knowledge Units database
├── obkg.redb          # Knowledge Graph
├── wallet.redb        # OBT wallet
├── identity.key       # Node identity
└── peers.json         # Remembered peers
```

### 7.2. Kết nối P2P

```bash
# Kết nối trực tiếp tới peer
onebrain start --seeds 192.168.1.10:4242,10.0.0.5:4242

# Kết nối trong REPL
> connect 192.168.1.10:4242
```

### 7.3. Bảo mật API token

⚠️ **Luôn thay đổi token mặc định trong production:**

```bash
# Tạo token ngẫu nhiên
# Linux/macOS:
TOKEN=$(openssl rand -hex 32)

# Windows PowerShell:
$TOKEN = -join ((48..57) + (65..90) + (97..122) | Get-Random -Count 32 | ForEach-Object {[char]$_})

# Khởi động với token bảo mật
onebrain start --api --api-token "$TOKEN"
```

### 7.4. CORS (Cross-Origin)

API server cho phép các origins:
- `http://localhost:3000`
- `http://localhost:5173` (Vite dev)
- `http://localhost:4173` (Vite preview)
- `http://localhost:8080`

Để thay đổi, sửa file `src/onebrain-api/src/server.rs`.

---

## 8. Kiến trúc hệ thống

```
┌──────────────────────────────────────────────────────────────┐
│                      USER (Browser)                          │
│               http://localhost:4280 (ALL-IN-ONE)             │
└──────────┬──────────────────────────────┬────────────────────┘
           │ Web Dashboard (/)            │ REST (/api/*)
           │ Static HTML/CSS/JS           │ WebSocket (/ws/*)
           ▼                              ▼
┌──────────────────────────────────────────────────────────────┐
│            onebrain start --api (1 process)                  │
│  ┌─────────────┐  ┌────────────┐  ┌───────────────────────┐ │
│  │  CLI REPL   │  │ API Server │  │  Static File Server   │ │
│  │  (stdin)    │  │ (axum)     │  │  (ServeDir)           │ │
│  └──────┬──────┘  └─────┬──────┘  └───────────┬───────────┘ │
│         └───────┬───────┘                      │             │
│                 ▼                              │             │
│          OneBrainNode ◄────────────────────────┘             │
│  ┌──────────┐ ┌──────────┐ ┌──────┐ ┌───────────────────┐   │
│  │ku-encoder│ │ku-mediator│ │ku-kql│ │    ku-net (P2P)   │   │
│  └──────────┘ └──────────┘ └──────┘ └─────────┬─────────┘   │
│  ┌──────────┐ ┌──────────┐                    │              │
│  │  ku-ai ──┼─┤ Ollama   │                    │              │
│  │          │ │ :11434   │                    │              │
│  └──────────┘ └──────────┘                    │              │
└───────────────────────────────────────────────┼──────────────┘
                                                │ TCP/QUIC
                                                ▼
                                    ┌───────────────────┐
                                    │   Other Nodes     │
                                    │  (Seed / Peer)    │
                                    └───────────────────┘
```

> **Nguyên tắc**: Mỗi user chạy node riêng, API riêng, Web Dashboard riêng.
> **Không có server tập trung.** Giữa các node giao tiếp P2P.

---

## 9. Gỡ cài đặt

### Linux / macOS

```bash
cd installer
./uninstall-linux.sh
```

### Windows

```powershell
cd installer
.\uninstall-windows.ps1
```

Dữ liệu knowledge tại `onebrain_data/` **không** bị xóa. Xóa thủ công nếu cần.

---

## 10. Xử lý lỗi thường gặp

### ❌ `Cannot connect` khi mở Web Dashboard

| Nguyên nhân | Giải pháp |
|-------------|-----------|
| API server chưa chạy | Chạy `onebrain start --api` trước |
| Sai token | Kiểm tra token trong terminal output |
| Port bị chiếm | Đổi port: `--api-port 4281` |

### ❌ `AI Unavailable` khi Encode/Chat

```bash
# Kiểm tra Ollama
ollama list              # Có model nào không?
ollama pull qwen3:8b     # Tải model nếu chưa có
ollama serve             # Đảm bảo Ollama đang chạy

# Kiểm tra kết nối
curl http://localhost:11434/api/tags
```

### ❌ `onebrain: command not found`

```bash
# Linux/macOS — thêm vào PATH:
export PATH="$HOME/.local/bin:$PATH"

# Windows — khởi động lại terminal sau install
```

### ❌ Build thất bại

```bash
# Cập nhật Rust
rustup update stable

# Xóa cache và build lại
cd src && cargo clean && cargo build --release -p onebrain-cli

# Web — xóa node_modules
cd src/onebrain-web
rm -rf node_modules package-lock.json
npm install && npm run build
```

### ❌ Model quá chậm

```bash
# Chuyển sang model nhỏ hơn
onebrain start --api --model qwen3:1.7b

# Hoặc đổi trong lúc chạy
> /model qwen3:1.7b
```

---

## Installer Scripts Reference

### Dành cho Developer (build & release)

| Script | Platform | Chức năng |
|--------|----------|-----------|
| **`release.sh`** | **Linux/macOS** | **Build + đóng gói `.tar.gz` để gửi cho user** |
| **`release.ps1`** | **Windows** | **Build + đóng gói `.zip` để gửi cho user** |
| `auto-install.sh` | Linux/macOS | One-command: build from source + install |
| `auto-install.ps1` | Windows | One-command: build from source + install |
| `build.sh` | Linux/macOS | Build only (auto-install prerequisites) |
| `build.ps1` | Windows | Build only (auto-install prerequisites) |

### Dành cho User (có sẵn trong package)

| Script | Platform | Chức năng |
|--------|----------|-----------|
| `install.sh` | Linux/macOS | **Chạy 1 file = cài xong** (trong `.tar.gz`) |
| `install.ps1` | Windows | **Chạy 1 file = cài xong** (trong `.zip`) |
| `uninstall.sh` | Linux/macOS | Gỡ cài đặt |
| `uninstall.ps1` | Windows | Gỡ cài đặt |

---

## Phân Phối Cho User

### Bước 1: Developer tạo release package

```bash
# Linux/macOS
cd installer && bash release.sh

# Windows
cd installer; .\release.ps1
```

Output:
```
release/
├── onebrain-linux-x86_64.tar.gz    # Gửi cho Linux user
├── onebrain-macos-arm64.tar.gz     # Gửi cho macOS user
└── onebrain-windows-x64.zip        # Gửi cho Windows user
```

### Bước 2: Gửi file cho user

Gửi 1 file `.zip` hoặc `.tar.gz` tương ứng với OS của user.

### Bước 3: User cài đặt

**Windows:**
```
1. Giải nén onebrain-windows-x64.zip
2. Right-click install.ps1 → "Run with PowerShell"
3. Done!
```

**Linux/macOS:**
```bash
tar xzf onebrain-linux-x86_64.tar.gz
cd onebrain-linux-x86_64
./install.sh
```

> **User KHÔNG cần cài Rust, Node.js hay bất kỳ build tool nào.**
> Chỉ cần Ollama cho AI features (installer sẽ hỏi cài).
