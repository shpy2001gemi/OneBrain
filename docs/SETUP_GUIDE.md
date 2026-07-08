# 🛠️ OneBrain — Hướng dẫn cài đặt & chạy

> **Phiên bản**: P10 (Pillar 10 — UI Layer)
> **Cập nhật**: 08/07/2026

---

## Mục lục

1. [Yêu cầu hệ thống](#1-yêu-cầu-hệ-thống)
2. [Cài đặt Rust Backend](#2-cài-đặt-rust-backend)
3. [Chạy OneBrain CLI](#3-chạy-onebrain-cli)
4. [Chạy API Server](#4-chạy-api-server)
5. [Cài đặt Web Dashboard](#5-cài-đặt-web-dashboard)
6. [Chạy toàn bộ hệ thống](#6-chạy-toàn-bộ-hệ-thống)
7. [Cấu hình](#7-cấu-hình)
8. [Kiểm tra hoạt động](#8-kiểm-tra-hoạt-động)
9. [Xử lý lỗi thường gặp](#9-xử-lý-lỗi-thường-gặp)

---

## 1. Yêu cầu hệ thống

### Bắt buộc

| Phần mềm | Phiên bản tối thiểu | Mục đích |
|-----------|---------------------|----------|
| **Rust** | 1.75+ | Biên dịch backend (ku-core, onebrain-node, onebrain-api, ...) |
| **Node.js** | 18+ | Chạy Web Dashboard (Vite dev server) |
| **npm** | 9+ | Quản lý dependencies JavaScript |

### Khuyến nghị

| Phần mềm | Phiên bản | Mục đích |
|-----------|-----------|----------|
| **Ollama** | 0.2+ | AI engine (local LLM) — cần cho tính năng Chat & Encode |
| **Git** | 2.30+ | Quản lý mã nguồn |

### Kiểm tra phiên bản

```bash
rustc --version        # rustc 1.75.0 trở lên
cargo --version        # cargo 1.75.0 trở lên
node --version         # v18.0.0 trở lên
npm --version          # 9.0.0 trở lên
ollama --version       # 0.2.0 trở lên (tùy chọn)
```

---

## 2. Cài đặt Rust Backend

### 2.1. Clone repository

```bash
git clone https://github.com/<your-org>/OneBrain.git
cd OneBrain/src
```

### 2.2. Biên dịch toàn bộ workspace

```bash
# Debug build (nhanh, để phát triển)
cargo build

# Release build (tối ưu, để deploy)
cargo build --release
```

### 2.3. Biên dịch riêng từng crate (nếu cần)

```bash
# Chỉ biên dịch API server
cargo build -p onebrain-api

# Chỉ biên dịch CLI
cargo build -p onebrain-cli

# Kiểm tra lỗi mà không tạo binary
cargo check -p onebrain-api
```

### 2.4. Cấu trúc workspace

```
OneBrain/src/
├── Cargo.toml            # Workspace root
├── ku-core/              # Core: types, storage, PoMV, OBT, CRDT
├── ku-net/               # P2P networking (TCP/QUIC, DHT, SWIM)
├── ku-kql/               # KQL query engine
├── ku-ai/                # AI backend (Ollama)
├── ku-encoder/           # Text → KU encoding
├── ku-mediator/          # Intent detection & response
├── onebrain-protocol/    # Shared P2P message types
├── onebrain-node/        # ★ Node runtime (shared by CLI, API, Desktop)
├── onebrain-cli/         # CLI REPL interface
├── onebrain-api/         # REST/WebSocket API server
├── onebrain-seed/        # Seed node
├── onebrain-web/         # React Web Dashboard
└── ku-demo/              # 3-node demo
```

---

## 3. Chạy OneBrain CLI

### 3.1. Chỉ CLI (không có Web Dashboard)

```bash
cd OneBrain/src
cargo run -p onebrain-cli -- start
```

### 3.2. CLI + Web Dashboard (khuyến nghị)

```bash
cd OneBrain/src
cargo run -p onebrain-cli -- start --api
```

Thêm `--api` sẽ khởi động API server trên cùng node, cho phép Web Dashboard kết nối.

### 3.3. Tùy chỉnh

```bash
cargo run -p onebrain-cli -- start \
  --name "MyBrain" \
  --port 4242 \
  --api \
  --api-port 4280 \
  --api-token "my-secret-token" \
  --ollama-url "http://localhost:11434" \
  --model "qwen3:8b"
```

| Flag | Mặc định | Mô tả |
|------|----------|-------|
| `--name` | `OneBrain` | Tên node |
| `--port` | `4242` | P2P port |
| `--api` | `false` | Bật API server cho Web Dashboard |
| `--api-port` | `4280` | Port API server |
| `--api-token` | `onebrain-dev-token` | Token xác thực |
| `--ollama-url` | `http://localhost:11434` | Ollama endpoint |
| `--model` | `qwen3:8b` | AI model |
| `--seeds` | — | Seed nodes (comma-separated) |

### 3.4. Các lệnh REPL

```
> encode "Photosynthesis converts light to chemical energy"
> search quantum computing
> status
> peers
> connect 192.168.1.10:4200
> kql FIND WHERE gene_type = "Fact" AND pomv > 0.5
> help
```

---

## 4. API Server (tích hợp trong CLI)

> **Kiến trúc**: API server **không** chạy riêng. Nó được tích hợp trong CLI thông qua flag `--api`.
> Cả CLI REPL và API server chia sẻ cùng 1 `OneBrainNode` instance → dữ liệu luôn đồng bộ.

```
┌─────────────────────────────────────────────────────────┐
│              onebrain start --api                       │
│  ┌────────────────────┐  ┌────────────────────────────┐ │
│  │  CLI REPL (stdin)  │  │  API Server (port 4280)    │ │
│  └─────────┬──────────┘  └──────────┬─────────────────┘ │
│            │                        │                   │
│            └──────┬─────────────────┘                   │
│                   ▼                                     │
│            OneBrainNode (shared)                        │
└─────────────────────────────────────────────────────────┘
```

### 4.1. Bật API

```bash
cargo run -p onebrain-cli -- start --api --api-token "your-secret-token"
```

> **⚠️ Quan trọng**: Thay `"your-secret-token"` bằng token bảo mật. Token mặc định cho dev là `onebrain-dev-token`.

### 4.2. API Server bind

- **Địa chỉ**: `127.0.0.1:4280` (chỉ localhost, không expose ra mạng)
- **CORS**: Cho phép `localhost:3000`, `localhost:5173`, `localhost:4173`, `localhost:8080`

### 4.3. Danh sách endpoints

| Nhóm | Method | Endpoint | Mô tả |
|------|--------|----------|--------|
| **Identity** | GET | `/api/identity` | Thông tin node identity |
| | POST | `/api/identity/recover` | Khôi phục identity từ recovery phrase |
| **Knowledge** | POST | `/api/encode` | Encode text → KU |
| | GET | `/api/kus` | Danh sách KUs (phân trang) |
| | GET | `/api/kus/{cid}` | Chi tiết 1 KU |
| | DELETE | `/api/kus/{cid}` | Xóa KU |
| | POST | `/api/search` | Full-text search |
| | POST | `/api/kql` | Chạy KQL query |
| **Chat** | POST | `/api/chat` | Chat qua Mediator |
| **Network** | GET | `/api/status` | Node status |
| | GET | `/api/peers` | Danh sách peers |
| | POST | `/api/peers/connect` | Kết nối peer mới |
| **Graph** | GET | `/api/graph/{cid}` | Graph visualization data |
| | GET | `/api/graph/{cid}/neighbors` | Neighbors trực tiếp |
| **Wallet** | GET | `/api/wallet` | OBT balance & info |
| | GET | `/api/wallet/history` | Lịch sử giao dịch |
| **Profile** | GET/PATCH | `/api/profile` | User profile |
| | GET/PATCH | `/api/settings` | Node settings |
| **AI** | GET | `/api/ai/status` | Kiểm tra kết nối Ollama |
| | GET | `/api/ai/models` | Danh sách models |
| | POST | `/api/ai/model` | Chuyển model |
| **Blob** | GET | `/api/blobs` | Danh sách blobs |
| | GET | `/api/blobs/{hash}` | Blob metadata |
| | DELETE | `/api/blobs/{hash}` | Xóa blob |
| | GET | `/api/blobs/stats` | Thống kê blob store |
| | POST | `/api/blobs/gc` | Garbage collection |
| **WebSocket** | GET | `/ws/events` | Real-time events stream |

### 4.4. Xác thực

Mỗi request REST cần header:
```
Authorization: Bearer <api-token>
```

WebSocket xác thực qua query parameter:
```
ws://127.0.0.1:4280/ws/events?token=<api-token>
```

---

## 5. Cài đặt Web Dashboard

### 5.1. Cài dependencies

```bash
cd OneBrain/src/onebrain-web
npm install
```

### 5.2. Chạy development server

```bash
npm run dev
```

Mở trình duyệt tại: **http://localhost:5173**

### 5.3. Build production

```bash
npm run build
npm run preview   # Preview bản build tại localhost:4173
```

### 5.4. Cấu trúc Web Dashboard

```
onebrain-web/src/
├── api/
│   ├── types.ts          # TypeScript types (khớp Rust API)
│   ├── client.ts         # HTTP API client (22+ endpoints)
│   └── ws.ts             # WebSocket client (auto-reconnect)
├── components/
│   ├── AppShell.tsx       # Layout wrapper (Sidebar + Header)
│   ├── Sidebar.tsx        # Navigation sidebar (8 items, collapsible)
│   ├── Header.tsx         # Top bar (page title, stats, OBT balance)
│   └── AuthGate.tsx       # Token authentication flow
├── pages/
│   ├── Dashboard.tsx      # #1 — Stats, recent KUs, AI status
│   ├── Explorer.tsx       # #2 — Search, filter, paginated KU table
│   ├── Encode.tsx         # #3 — Text → KU encoding
│   ├── Chat.tsx           # #4 — Chat interface
│   ├── Graph.tsx          # #5 — SVG force-directed graph
│   ├── Pomv.tsx           # #6 — PoMV 6-dimension breakdown
│   ├── NetworkPage.tsx    # #7 — Peer list, connect form
│   └── Wallet.tsx         # #8 — OBT balance, earnings, transactions
├── App.tsx                # Router (8 routes)
├── main.tsx               # Entry point
└── index.css              # Design system (dark glassmorphism)
```

### 5.5. Đăng nhập Web Dashboard

Khi mở Web Dashboard lần đầu, bạn sẽ thấy màn hình đăng nhập:

1. Nhập **API Token** (cùng token cấu hình ở bước 4.1)
2. Nhấn **Connect**
3. Dashboard sẽ tự động kết nối tới `http://127.0.0.1:4280`

Token được lưu trong `localStorage` → lần sau không cần nhập lại.

---

## 6. Chạy toàn bộ hệ thống

### Bước 1: Khởi động Ollama (tùy chọn — cần cho Chat & Encode)

```bash
ollama serve
# Trong terminal khác:
ollama pull llama3.2      # Hoặc model khác
```

### Bước 2: Khởi động OneBrain (CLI + API server)

```bash
cd OneBrain/src
cargo run -p onebrain-cli -- start --api
```

> CLI REPL + API server chạy cùng process, chia sẻ cùng 1 node.
> API endpoint: `http://127.0.0.1:4280`
> Token mặc định: `onebrain-dev-token`

### Bước 3: Khởi động Web Dashboard

```bash
cd OneBrain/src/onebrain-web
npm run dev
```

> Web Dashboard: `http://localhost:5173`

### Bước 4: Mở trình duyệt

```
http://localhost:5173
```

Nhập API token → bắt đầu sử dụng!

### Tóm tắt

```
┌─────────────────────────────────────────────────────┐
│  Terminal 1: ollama serve                (tùy chọn) │
│  Terminal 2: onebrain start --api        (CLI+API)  │
│  Terminal 3: cd onebrain-web && npm run dev  (Web)   │
│  Browser:    http://localhost:5173                   │
└─────────────────────────────────────────────────────┘
```

> **Lưu ý**: Chỉ cần 2 terminal nếu Ollama đã chạy sẵn hoặc không cần AI.

---

## 7. Cấu hình

### 7.1. Cấu hình Node (`config.toml`)

```toml
name = "MyBrain"
port = 4200                          # P2P port
data_dir = "~/.onebrain/data"
ollama_url = "http://127.0.0.1:11434"
model = "llama3.2"

[seeds]
addresses = ["seed1.onebrain.io:4200"]

[identity]
path = "~/.onebrain/identity.key"
```

### 7.2. Biến môi trường

| Biến | Mặc định | Mô tả |
|------|----------|--------|
| `ONEBRAIN_API_PORT` | `4280` | Port API server |
| `ONEBRAIN_API_TOKEN` | — | Bearer token xác thực |
| `OLLAMA_HOST` | `http://127.0.0.1:11434` | Ollama API endpoint |

### 7.3. CORS

API server cho phép các origin sau:
- `http://localhost:3000`
- `http://127.0.0.1:5173` (Vite dev)
- `http://localhost:4173` (Vite preview)
- `http://127.0.0.1:8080`

Nếu cần thêm origin, sửa trong `onebrain-api/src/server.rs`.

---

## 8. Kiểm tra hoạt động

### 8.1. Test API bằng curl

```bash
# Kiểm tra status
curl -H "Authorization: Bearer <token>" http://127.0.0.1:4280/api/status

# Encode knowledge
curl -X POST http://127.0.0.1:4280/api/encode \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{"text": "Photosynthesis converts light energy into chemical energy"}'

# Chat
curl -X POST http://127.0.0.1:4280/api/chat \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{"message": "What do you know about photosynthesis?"}'

# Danh sách KUs
curl -H "Authorization: Bearer <token>" "http://127.0.0.1:4280/api/kus?page=1&limit=10"
```

### 8.2. Test WebSocket

```javascript
const ws = new WebSocket("ws://127.0.0.1:4280/ws/events?token=<token>");
ws.onmessage = (e) => console.log(JSON.parse(e.data));
```

### 8.3. Kiểm tra build

```bash
# Rust
cd OneBrain/src
cargo check                       # Kiểm tra lỗi compile
cargo test                        # Chạy unit tests
cargo check -p onebrain-api       # Chỉ kiểm tra API crate

# Web
cd OneBrain/src/onebrain-web
npx tsc --noEmit                  # Kiểm tra TypeScript
npm run build                     # Build production bundle
```

---

## 9. Xử lý lỗi thường gặp

### ❌ `Cannot connect` khi đăng nhập Web Dashboard

**Nguyên nhân**: API server chưa chạy, hoặc sai token.

**Giải pháp**:
1. Kiểm tra API server đang chạy: `curl http://127.0.0.1:4280/api/status`
2. Kiểm tra đúng token
3. Kiểm tra port 4280 không bị chặn

### ❌ `CORS error` trong console trình duyệt

**Nguyên nhân**: Origin của Web Dashboard không nằm trong danh sách CORS.

**Giải pháp**: Web Dashboard phải chạy trên một trong các port: 3000, 5173, 4173, 8080.

### ❌ `AI Unavailable` khi Encode/Chat

**Nguyên nhân**: Ollama không chạy hoặc model chưa pull.

**Giải pháp**:
```bash
ollama serve               # Khởi động Ollama
ollama list                # Kiểm tra models đã cài
ollama pull llama3.2       # Cài model nếu chưa có
```

### ❌ `cargo build` thất bại

**Nguyên nhân**: Thiếu toolchain hoặc dependency.

**Giải pháp**:
```bash
rustup update stable
rustup default stable
cargo clean
cargo build
```

### ❌ `npm install` thất bại

**Giải pháp**:
```bash
rm -rf node_modules package-lock.json
npm install
```

---

## Ghi chú kiến trúc

```
┌─────────────────────────────────────────────────────────────┐
│                     USER (Browser)                          │
│             http://localhost:4280 (ALL-IN-ONE)               │
│         Web Dashboard + REST API + WebSocket                 │
└─────────────┬───────────────────────────────┬───────────────┘
              │ REST (/api/*)                 │ WS (/ws/*)
              │ Static (/)                    │
              ▼                               ▼
┌─────────────────────────────────────────────────────────────┐
│         onebrain start --api (axum, localhost:4280)          │
│         Bearer Token Auth + Static File Serving              │
├─────────────────────────────────────────────────────────────┤
│                    OneBrainNode                             │
│  ┌──────────┐ ┌──────────┐ ┌───────┐ ┌──────────────────┐  │
│  │ku-encoder│ │ku-mediator│ │ku-kql │ │     ku-net       │  │
│  └──────────┘ └──────────┘ └───────┘ │  P2P (TCP/QUIC)  │  │
│  ┌──────────┐ ┌──────────┐           └────────┬─────────┘  │
│  │  ku-ai   │ │ ku-core  │                    │             │
│  │ (Ollama) │ │ (redb)   │                    │             │
│  └──────────┘ └──────────┘                    │             │
└───────────────────────────────────────────────┼─────────────┘
                                                │ P2P
                                                ▼
                                    ┌───────────────────┐
                                    │   Other Nodes     │
                                    │  (Seed / Peer)    │
                                    └───────────────────┘
```

> **Nguyên tắc**: Mỗi user chạy node riêng → API riêng → Web Dashboard riêng. **Không có server tập trung.** Giữa các node giao tiếp P2P.

---

## 10. Cài đặt nhanh bằng Installer

Xem chi tiết tại [installer/README.md](../installer/README.md).

### Windows
```powershell
cd installer
.\build.ps1            # Build CLI + Web
.\install-windows.ps1  # Cài vào %LOCALAPPDATA%\OneBrain
onebrain start --api   # Chạy!
```

### Linux
```bash
cd installer && chmod +x *.sh
./build.sh             # Build CLI + Web
./install-linux.sh     # Cài vào ~/.local
onebrain-dashboard     # Chạy!
```

### macOS
```bash
cd installer && chmod +x *.sh
./build.sh             # Build CLI + Web
./install-macos.sh     # Cài vào ~/.local
onebrain-dashboard     # Chạy!
```

Sau khi cài, mở **http://localhost:4280** — tất cả (Web Dashboard + API + WebSocket) chạy trên 1 port duy nhất.

