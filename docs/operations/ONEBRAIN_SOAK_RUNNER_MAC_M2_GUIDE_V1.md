# OneBrain M5-07 — Portable runner cho Mac mini M2

Tài liệu này cài runner GitHub Actions dạng portable trên macOS ARM64. Runner
không cài LaunchDaemon/LaunchAgent, mặc định là **ephemeral**: nhận đúng một job,
tự deregister và thoát.

## 1. Cấu hình khuyến nghị

- Mac mini Apple Silicon M1/M2/M3/M4;
- macOS 14 trở lên, cập nhật security patch hiện hành;
- ít nhất 8 GiB RAM và 50 GiB SSD trống;
- Ethernet hoặc Wi-Fi ổn định;
- nguồn điện và Internet ổn định ít nhất 4 ngày;
- một macOS user riêng, không chứa SSH key, ví, cloud credential hoặc dữ liệu
  cá nhân quan trọng.

GitHub gắn nhãn mặc định `self-hosted`, `macOS`, `ARM64`; kit bổ sung nhãn
`onebrain-soak-macos-arm64`.

Repository hiện là public. Chỉ workflow manual, branch `main`, permission
`contents: read` mới được route đến runner Mac. Ephemeral mode vẫn là mặc định.

## 2. Port và firewall

Không cần mở TCP/UDP inbound. Runner chỉ cần outbound HTTPS TCP 443 tới GitHub.
Real QUIC của M5-07 bind loopback trên chính Mac.

Nếu firewall/application filter dùng allowlist, cho phép các domain trong tài
liệu chính thức của GitHub:

- `github.com`;
- `api.github.com`;
- `codeload.github.com`;
- `*.actions.githubusercontent.com`;
- `results-receiver.actions.githubusercontent.com`;
- `release-assets.githubusercontent.com`.

## 3. Kiểm tra native Apple Silicon

Mở Terminal và chạy:

```bash
uname -s
uname -m
```

Kết quả bắt buộc:

```text
Darwin
arm64
```

Nếu `uname -m` trả `x86_64`, Terminal đang chạy qua Rosetta. Mở Finder →
Applications → Utilities → Terminal → Get Info, bỏ **Open using Rosetta**, rồi
mở Terminal mới.

Không chạy runner bằng `root` hoặc `sudo`.

## 4. Dependency

Apple Xcode Command Line Tools và Homebrew cung cấp compiler cùng các build
tools cần thiết.

Cài Command Line Tools:

```bash
xcode-select --install
```

Nếu chưa có Homebrew, cài từ trang chính thức:

<https://brew.sh>

Kit dùng các formula:

```text
python@3.13
cmake
pkgconf
```

Lệnh `deps` tự cài hoặc cập nhật các formula này; runner GitHub vẫn nằm portable
trong user home.

## 5. Tải runner kit

```bash
mkdir -p ~/onebrain-runner-kit
cd ~/onebrain-runner-kit

curl -fL \
  https://raw.githubusercontent.com/shpy2001gemi/OneBrain/main/scripts/runner/onebrain-soak-runner.sh \
  -o onebrain-soak-runner.sh

chmod +x onebrain-soak-runner.sh
```

Chạy dependency setup và preflight:

```bash
./onebrain-soak-runner.sh deps
./onebrain-soak-runner.sh doctor
```

`doctor` phải nhận diện:

```text
macOS ARM64 (Darwin/arm64)
```

## 6. Lấy registration token

Trên GitHub:

1. Mở repository `shpy2001gemi/OneBrain`.
2. Chọn **Settings → Actions → Runners**.
3. Chọn **New self-hosted runner**.
4. Chọn **macOS** và **ARM64**.
5. Chỉ copy giá trị sau `--token`.

Token có hiệu lực ngắn. Không gửi token qua chat, không ghi vào file và không
commit.

## 7. Bấm chạy

Mở menu:

```bash
./onebrain-soak-runner.sh
```

Chọn:

```text
2) First-time ephemeral setup and run
```

Hoặc chạy thẳng:

```bash
./onebrain-soak-runner.sh setup-run
```

Kit sẽ:

1. kiểm tra native macOS ARM64;
2. lấy asset `actions-runner-osx-arm64` mới nhất;
3. xác minh SHA-256 từ GitHub release metadata;
4. đăng ký nhãn `onebrain-soak-macos-arm64`;
5. chạy qua `caffeinate` để Mac không sleep;
6. nhận một job, tự deregister và thoát.

Giữ macOS user đăng nhập trong thời gian chạy. Không logout, shutdown hoặc cài
OS update giữa job 24/72 giờ.

## 8. Khởi động workflow

Trên GitHub:

1. Mở **Actions**.
2. Chọn **vNext soak macOS ARM64**.
3. Chọn **Run workflow**.
4. Chọn branch `main`.
5. Chọn `nightly-24h` hoặc `pre-release-72h`.
6. Chọn **Run workflow**.

Có thể queue workflow trước rồi bật runner. Job chỉ nhận runner có đủ bốn nhãn:

```text
self-hosted, macOS, ARM64, onebrain-soak-macos-arm64
```

Artifact kết quả có tên bắt đầu bằng:

```text
vnext-soak-macos-arm64-
```

Report JSON ghi cả `host_os` và `host_arch`, đồng thời dùng `libproc` của Apple
để đo RSS và thread count thật.

## 9. Chạy nền và bật/tắt

Nếu runner đã đăng ký:

```bash
./onebrain-soak-runner.sh start
./onebrain-soak-runner.sh status
./onebrain-soak-runner.sh logs
```

Tắt:

```bash
./onebrain-soak-runner.sh stop
```

Script yêu cầu gõ `STOP`; dừng trong lúc soak sẽ làm job thất bại.

Persistent mode không cài service:

```bash
./onebrain-soak-runner.sh setup --persistent
./onebrain-soak-runner.sh start
```

Sau reboot phải chạy lại `start`. Với repo public, ưu tiên ephemeral mode.

## 10. Gỡ bỏ

Nếu runner còn đăng ký trên GitHub, mở:

**Settings → Actions → Runners → tên runner → Remove**

Copy removal token rồi chạy:

```bash
./onebrain-soak-runner.sh remove
```

Xóa toàn bộ binary, cache, workspace và log local:

```bash
./onebrain-soak-runner.sh purge
```

Gõ `PURGE` để xác nhận. Gỡ đăng ký và xóa local trong một lần:

```bash
./onebrain-soak-runner.sh uninstall
```

Runner home mặc định:

```text
~/Library/Application Support/OneBrain/actions-runner
```

Ephemeral runner đã hoàn thành job thường tự deregister. Khi đó chỉ cần kiểm tra
runner không còn trong GitHub Settings rồi chạy `purge`.

## 11. Troubleshooting

### Terminal báo x86_64

Tắt Rosetta cho Terminal và mở terminal mới. Không dùng runner x64 giả lập trên
Apple Silicon để tạo performance evidence.

### Thiếu Xcode tools

```bash
xcode-select --install
```

Đợi installer hoàn tất rồi chạy lại `deps`.

### Thiếu Homebrew hoặc formula

Cài Homebrew từ <https://brew.sh>, rồi chạy:

```bash
./onebrain-soak-runner.sh deps
./onebrain-soak-runner.sh doctor
```

### Job queued

Kiểm tra:

```bash
./onebrain-soak-runner.sh status
./onebrain-soak-runner.sh logs
```

Runner phải online và có nhãn `onebrain-soak-macos-arm64`. Workflow phải chạy từ
branch `main`.

### Mac sleep hoặc mất điện

Kit và workflow đều dùng `caffeinate`, nhưng không chống shutdown, logout, mất
điện hoặc OS update. Dùng nguồn ổn định và tắt lịch tự động restart/update trong
thời gian qualification.
