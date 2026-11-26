# 🛠️ OneBrain — Hướng dẫn cài đặt & chạy

> **Phiên bản**: P10 & vNext `/ku` Workspace
> **Cập nhật**: 26/11/2026

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
| **Rust** | 1.85+ (stable; CI khóa `1.100`) | Biên dịch backend (`ku-core`, `onebrain-node`, `onebrain-api`, `onebrain-cli`, ...) |
| **Node.js** | `20.19+` hoặc `22.12+` | Build & chạy Web Dashboard (Vite 8) |
| **npm** | 10+ | Quản lý dependencies JavaScript |
| **Python** | 3.10+ | Chạy kiểm tra contract và script bootstrap demo (`scripts/base/bootstrap_ku_dev_demo.py`) |

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
├── Cargo.toml                  # Workspace root (21 crate Rust)
├── ku-core/                    # Foundation canonical vNext, KU semantics, Vault + legacy Core DNA/PoMV/OBT
├── ku-net/                     # Phiên QUIC/OBP reconciliation (feature-gated) + legacy DHT/SWIM
├── ku-kql/                     # Chỉ mục vnext_semantic_index, KQL chỉ-đọc local + experimental discovery
├── ku-ai/                      # AI backend policies & Ollama adapter
├── ku-encoder/                 # Khung trích xuất KU dùng chung, semantic selection v1/v2, review lowering
├── ku-mediator/                # Intent detection & response
├── onebrain-protocol/          # Shared P2P message types & canonical codecs
├── onebrain-relay/             # Outbound-first QUIC/TLS-443 relay service
├── onebrain-node/              # ★ Node runtime độc lập host (dùng chung cho CLI, API, Desktop)
├── onebrain-cli/               # CLI (`start`, `ku`, `obp`, `need`, `pomv`, `vnext`)
├── onebrain-api/               # REST/WebSocket API server & example `ku_local_web`
├── onebrain-desktop/           # Tauri 2.x Desktop App
├── onebrain-mobile-core/       # Autonomous Rust mobile runtime profile
├── onebrain-mobile-bridge/     # C ABI/JNI bridge cho Flutter NativeHost
├── onebrain-archive/           # Encrypted Base `OBARV002` archive container dùng chung
├── onebrain-blob-store/        # Persistent hybrid redb + filesystem Base blob store
├── onebrain-legacy-ku-store/   # Kho tương thích legacy Core DNA `KuStorage` + `GraphStorage`
├── onebrain-base-contract/     # Generated Base v1 semantic contract (Rust/TS/Dart)
├── onebrain-base-abi/          # Stable Base v1 C ABI projection
├── onebrain-seed/              # Daemon tương thích TCP/JSON legacy
├── ku-demo/                    # Multi-node simulation demo
├── onebrain-web/               # React + Vite + TypeScript Web UI (`/ku` workspace + dashboard)
└── onebrain-mobile/            # Flutter Android/iOS autonomous mobile node
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
  --api-token-file "<path/to/api-token.txt>" \
  --ollama-url "http://localhost:11434" \
  --model "qwen3:8b"
```

| Flag | Mặc định | Mô tả |
|------|----------|-------|
| `--name` | `OneBrain` | Tên node |
| `--port` | `4242` | P2P port |
| `--api` | `false` | Bật API server cho Web Dashboard |
| `--api-port` | `4280` | Port API server |
| `--api-token-file` | — | File UTF-8 riêng tư chứa Bearer token (32..1024 ký tự ASCII chữ/số/`-`/`_`; xung đột với `--api-token`) |
| `--api-token` | `onebrain-dev-token` | Token xác thực truyền trực tiếp (chỉ nên dùng cho dev nhanh; ưu tiên `--api-token-file`) |
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
cargo run -p onebrain-cli -- start --api --api-token-file "<path/to/api-token.txt>"
```

> **⚠️ Quan trọng**: Ưu tiên `--api-token-file` (chứa đúng một token UTF-8 dài 32..1024 ký tự ASCII chữ/số/`-`/`_`, không đi cùng `--api-token`) cho cả `onebrain start --api` lẫn các lệnh client CLI (`onebrain ku`, `onebrain obp`, `onebrain need`, `onebrain pomv`, `onebrain vnext`, ưu tiên trước `ONEBRAIN_API_TOKEN`) để token không lộ trong lịch sử shell hay đối số tiến trình. Token mặc định khi chạy `onebrain start --api` dev nhanh không cờ là `onebrain-dev-token` (hoặc `--api-token "..."`). Host CLI không in giá trị Bearer token thô ra stdout.

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

Bản build production gọi API theo origin đã phục vụ trang (host `onebrain start --api`
hoặc `ku_local_web` phục vụ Web và API trên cùng một cổng, cổng nào cũng được).
Khi xem bằng `npm run preview` (API ở cổng khác), build với
`VITE_API_SAME_ORIGIN=false` để dùng `VITE_API_BASE` (mặc định `http://127.0.0.1:4280`).
Chế độ `npm run dev` luôn dùng `VITE_API_BASE`.
Trang `/ku` (Local KU workspace) cần KU runtime. Host `ku_local_web` luôn có; với
`onebrain start --api` thêm Vault key của dataset và Registry đã ký, ví dụ:

```bash
cargo run -p onebrain-cli -- start --api --data-dir <dataset> \
  --api-token-file <api-token.txt> \
  --concept-registry-release-root <registry-root> \
  --concept-registry-release-public-key <64-hex-public-key> \
  --ku-vault-key-file <vault.key> \
  --ku-source "<label>=<source.canonical>"   # tùy chọn, lặp được, tối đa 64
```

Không có `--ku-vault-key-file` thì `/ku` vẫn mở nhưng báo `Host: unavailable` và
`ku_dependency_unavailable` (HTTP 503). Mặc định host CLI chỉ lưu local/private và
không bật Ollama. Muốn có AI intake/bản nháp nền trên host CLI, thêm cờ opt-in (model
phải cài sẵn, host không tải model; model lỗi chỉ là cảnh báo, sửa tay và đọc KU vẫn chạy):

```bash
  --ku-ollama-executable <ollama.exe> --ku-ollama-models-dir <models-dir> \
  --ku-ollama-model qwen3:8b            # lặp được, 1..8; tùy chọn --ku-ollama-memory-limit-bytes (mặc định 12 GiB)
```

Để host CLI cũng chia sẻ/duyệt/xem Public KU D-045, build với
`--features vnext-network-runtime` và thêm cờ opt-in loopback (tắt mặc định; mặc
định chỉ nhận địa chỉ loopback; muốn thử giữa hai máy thật trên cùng mạng LAN riêng
thì thêm cờ opt-in thứ hai `--ku-network-allow-lan` cho địa chỉ IPv4 RFC 1918 hoặc
IPv6 `fc00::/7` trong khi API/Web vẫn khóa ở `127.0.0.1` — `DEV-ONLY`, `D-063`; không
dùng chung với `--vnext-kql`/`--vnext-public-use`/`--vnext-pomv-view`):

```bash
cargo run -p onebrain-cli --features vnext-network-runtime -- start --api ... \
  --ku-vault-key-file <vault.key> --ku-network-bind 127.0.0.1:0
```

Host in `Opt-in loopback OBP-RP: node_id=… listen_addr=…` (hoặc cảnh báo
`Opt-in private-LAN OBP-RP (DEV-ONLY, D-063)` khi bật `--ku-network-allow-lan`) —
đó là NodeID/địa chỉ mà host bên kia điền vào bước chia sẻ; trang `/ku` cũng hiện
NodeID này, hướng dẫn điền `Selector CID` / `Namespace commitment` thống nhất giữa
hai bên (kèm nút điền mẫu DEV-ONLY khi thử nghiệm), và badge header (số KU private
đã lưu, trạng thái mạng loopback hoặc `Private-LAN (DEV-ONLY)`). Bản chuẩn bị chia
sẻ (`share_prepare`) chưa hết hạn được niêm phong vào journal `ku_public_share_prepared_v1`
nên giữ nguyên qua lần khởi động lại host trước khi confirm (`D-062`).

Ngoài giao diện `/ku` Web, client CLI hỗ trợ đầy đủ thao tác chia sẻ/duyệt/tra nhãn
qua `--api-token-file` (`onebrain ku share prepare|preview|confirm|intent|inspect|list` —
lệnh `confirm` tự đọc lại `share_preview` và bắt gõ đúng `prepared_id`, `D-059`;
`onebrain ku describe --ccid <32-hex>` tra ngược nhãn Registry chỉ để hiển thị,
`D-058`). Trên host nhận (kể cả sau khi khởi động lại ở chế độ local-only offline
không bật `--ku-network-bind`), client CLI cũng đọc và xuất Public KU đã nhận:

```bash
cargo run -p onebrain-cli -- ku --api-url http://127.0.0.1:4280 \
  --api-token-file <api-token.txt> get --object-cid <64-hex-public-cid>

cargo run -p onebrain-cli -- ku --api-url http://127.0.0.1:4280 \
  --api-token-file <api-token.txt> describe --ccid <32-hex-ccid>

# export.json: {"mode":"canonical_public_exchange","object_cids":["<64-hex-public-cid>"]}
cargo run -p onebrain-cli -- ku --api-url http://127.0.0.1:4280 \
  --api-token-file <api-token.txt> export --payload-file <export.json>
```

Script kiểm tra lặp lại hai tiến trình `python scripts/base/check_ku_public_share_demo.py`
hỗ trợ `--sender-kind web|cli`, `--receiver-kind peer|web|cli` và `--cli-share` (chạy
luồng chia sẻ qua `onebrain ku share`, tự restart sender giữa `share_prepare` và
`share_confirm` để kiểm chứng `D-062`, kiểm tra `onebrain ku get`/`export`/`describe
--api-token-file` và chế độ offline trên receiver Web/CLI).
Đây là demo cục bộ/LAN riêng DEV-ONLY, không phải NAT/production qualification; không có reward.

#### Chạy `/ku` lần đầu từ fresh clone bằng một lệnh (D-056)

Host `/ku` bắt buộc có Concept Registry đã ký + kích hoạt; repo không phát hành
Registry thật. Để contributor thử ngay, lệnh bootstrap kiểm tra prerequisite rồi tạo
toàn bộ đầu vào demo **ngoài repo** (thư mục `--demo-root` phải chưa tồn tại):

```powershell
python scripts/base/bootstrap_ku_dev_demo.py --check-only          # chỉ kiểm tra Python/cargo/npm/Web dist
python scripts/base/bootstrap_ku_dev_demo.py --demo-root C:/OneBrainLocal/dev-demo --dev-demo-registry
```

Linux/macOS (Ubuntu/Debian mới không có lệnh `python`; cần thêm `build-essential`
cho Rust và Node.js 20.19+/22.12+ cho Vite 8):

```bash
python3 scripts/base/bootstrap_ku_dev_demo.py --check-only
python3 scripts/base/bootstrap_ku_dev_demo.py --demo-root ~/OneBrainLocal/dev-demo --dev-demo-registry
```

Các bước tiếp được in theo shell của nền tảng: PowerShell trên Windows, POSIX shell
(`python3`, `&&`, quote kiểu `sh`) ở nơi khác, dán nguyên dòng chạy được.

Lệnh tạo: Registry tổng hợp **DEV-ONLY** (water, liquid, solid, gas, ice, sun, earth,
cat, tree + nhãn tiếng Việt; signer DEV sinh từ OS random, chỉ giữ trong bộ nhớ rồi
bỏ), Vault key + API token (`prepare_ku_local_secrets.py`), nguồn private mẫu
`Water is a liquid.` (`ku_manual_source`) và `host.json`; sau đó in các bước tiếp
(build Web nếu thiếu, `cargo run ... --example ku_local_web -- <host.json>`, mở
`http://127.0.0.1:4280/ku`, token đọc tại `sender/secrets/api-token.txt`, tự
preview rồi bấm **Save exact preview privately**). Lệnh không in token/key, không
ghi đè file, không lưu/chia sẻ KU và không khởi động host. Bước in ra cũng có lệnh
host CLI thay thế (`cargo run -p onebrain-cli ... -- start --api --api-token-file
... --ku-source ... --web-dir ...`, cùng dữ liệu/cổng/`/ku`). Thêm `--with-receiver`
để có `receiver.json` + khối `"network"` loopback opt-in cho cả hai (hoặc thêm
`--lan-ip <ip-lan-riêng>` cùng `--with-receiver` để sinh cấu hình `"allow_private_lan": true`
 / `--ku-network-allow-lan` khi thử chia sẻ DEV-ONLY qua mạng LAN riêng — `D-063`;
receiver cũng admit nguồn mẫu như nguồn riêng tư của chính nó, không lưu gì, để trên
`/ku` của receiver nút **Start a new private draft from …** ở `Browse accepted Public KUs`
điền sẵn text vào draft manual riêng tư mới; vẫn tự chọn nguồn, concept, preview và
Save — D-057), rồi chạy
`check_ku_public_share_demo.py` với `--receiver-kind web` (host Web) hoặc
`--sender-kind cli --receiver-kind cli --require-cli-client --cli-share` (host CLI +
`onebrain ku share` + `onebrain ku get/export/describe`; build `-p onebrain-cli
--features vnext-network-runtime`). Cờ
tường minh `--seed-sample-ku` chỉ khi sender chưa có KU nào sẽ lưu riêng tư KU mẫu
qua đúng API của nút `/ku` (reserve → draft → preview → save, `published=false`);
không có cờ này thì phải tự lưu KU trước. CI
[ku-dev-demo-scripts.yml](../.github/workflows/ku-dev-demo-scripts.yml) chạy unit
test các script và `--check-only` trên Ubuntu/Windows, cộng job Linux/macOS (bash) và job Windows (dòng PowerShell được in ra) chạy lại
đúng first run này (build Web, bootstrap DEV-ONLY vào demo root tạm có dấu cách,
chạy nguyên văn dòng check CLI/CLI được in ra, gồm `cargo build` + `--seed-sample-ku`,
rồi dòng check Web/Web được in ra, gồm build example `ku_local_web`, chạy trên demo
root tạm thứ hai bootstrap riêng để `--seed-sample-ku` lưu KU mẫu qua Web host; mỗi
bước fail nếu seed của chính nó không PASS);
workflow tự chạy khi PR/push vào main đổi các script/workflow đó, `/ku` Web
(`src/onebrain-web`, trừ Markdown), example `ku_local_web` + file host/API `/ku`
dùng chung, hoặc `onebrain start`/`onebrain ku` của CLI; các crate runtime sâu hơn
dựa vào lần chạy hằng tuần để bắt lệch runtime (hoặc chạy tay `workflow_dispatch`).

Giới hạn: Registry DEV-ONLY **không** phải trust Registry của operator, CCID không
tương thích với bản phát hành thật, KU lưu trên đó chỉ là dữ liệu demo. Không có
`--dev-demo-registry` thì lệnh vẫn đòi Registry thật đã xác minh/kích hoạt:
`--registry-root <root> --registry-public-key-file <key.txt>` (chạy `status` chỉ đọc
trước khi tạo secrets/nguồn/config). Lỗi có mã `ku_dev_demo_*` kèm hướng dẫn; thư
mục demo dở dang được giữ lại, thử lại với `--demo-root` mới.

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
| `ONEBRAIN_API_TOKEN` | — | Bearer token xác thực cho các lệnh client CLI khi không truyền `--api-token-file` hoặc `--api-token` |
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

