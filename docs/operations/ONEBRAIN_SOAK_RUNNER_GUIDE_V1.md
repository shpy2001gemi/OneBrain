# OneBrain M5-07 — Portable Linux Soak Runner Guide v1

Tài liệu này thiết lập một GitHub Actions self-hosted runner Linux x64 dành
riêng cho workflow `vNext soak and release gate`. Runner mặc định ở chế độ
**ephemeral**: nhận đúng một job, tự deregister sau job và thoát. Không cài
systemd service và không chạy thường trực khi không cần.

Script:
[`onebrain-soak-runner.sh`](../../scripts/runner/onebrain-soak-runner.sh)

## 1. Phạm vi và cảnh báo bảo mật

Repository OneBrain đang public. GitHub khuyến cáo không dùng self-hosted
runner lâu dài cho public repository vì code không tin cậy có thể chiếm runner.
Bộ này giảm bề mặt rủi ro bằng các ràng buộc:

- long-soak chỉ chạy từ `main`, không chạy từ pull request;
- workflow chỉ có `contents: read`;
- runner mặc định ephemeral, một job rồi tự deregister;
- không dùng systemd và không chạy dưới `root`;
- token đăng ký/xóa được nhập ẩn và chỉ có hiệu lực ngắn;
- runner nên nằm trên VM/máy riêng, không chứa SSH key, cloud credential, ví,
  dữ liệu cá nhân hoặc quyền truy cập mạng nội bộ nhạy cảm.

Không dùng máy production hoặc máy làm việc chính làm runner.

Tham khảo:

- [GitHub secure use reference](https://docs.github.com/en/actions/reference/security/secure-use)
- [Self-hosted runner reference](https://docs.github.com/en/actions/reference/runners/self-hosted-runners)

## 2. Cấu hình máy

Khuyến nghị:

- Ubuntu 22.04/24.04, Debian tương đương, Rocky/Alma/RHEL 8+ x64;
- 4 vCPU, 8 GiB RAM;
- SSD từ 50 GiB;
- đồng hồ NTP đồng bộ;
- Internet và nguồn điện ổn định ít nhất 4 ngày;
- một user Linux không có dữ liệu/credential quan trọng.

M5-07 chạy QUIC thật trên loopback, build Rust release, đo fsync/RSS/disk/task
và giữ job chạy 24 hoặc 72 giờ. Không chạy workload nặng khác trên máy trong
thời gian soak.

CentOS 7 không được hỗ trợ: hệ điều hành này đã EOL và GitHub Actions yêu cầu
CentOS/RHEL 8 trở lên. Không đổi sang mirror archive/vault để chạy soak; hãy
migrate máy sang Rocky/Alma 9, RHEL 9 hoặc Ubuntu 24.04 trước.

## 3. Firewall và port

### Không mở inbound port cho OneBrain

Không cần mở TCP/UDP inbound cho runner hoặc QUIC:

- GitHub runner chủ động kết nối ra ngoài;
- M5-07 dùng `127.0.0.1` và cổng QUIC tạm thời nội bộ;
- không expose OneBrain node ra Internet.

Cloud security group nên chỉ giữ SSH từ IP quản trị của bạn. Không thêm rule
inbound cho `onebrain-soak`.

### Outbound bắt buộc

GitHub yêu cầu outbound HTTPS trên TCP 443. Nếu firewall mặc định cho phép
outbound, không cần thay đổi gì.

Nếu UFW đang dùng chính sách chặn outbound:

```bash
sudo ufw allow out 443/tcp
```

Không bật UFW trên máy remote trước khi cho phép cổng SSH thực tế. Ví dụ với
SSH mặc định:

```bash
sudo ufw allow 22/tcp
sudo ufw allow out 443/tcp
sudo ufw status verbose
```

Nếu SSH dùng cổng khác, thay `22` bằng cổng đó. Các firewall lọc theo domain
cần cho phép tối thiểu:

```text
github.com
api.github.com
*.actions.githubusercontent.com
codeload.github.com
results-receiver.actions.githubusercontent.com
*.blob.core.windows.net
objects.githubusercontent.com
objects-origin.githubusercontent.com
github-releases.githubusercontent.com
github-registry-files.githubusercontent.com
release-assets.githubusercontent.com
```

Danh sách canonical và lưu ý CNAME nằm trong
[GitHub communication requirements](https://docs.github.com/en/actions/reference/runners/self-hosted-runners#communication).

## 4. Chuẩn bị script

### Cách A — Clone repository

```bash
git clone https://github.com/shpy2001gemi/OneBrain.git
cd OneBrain
chmod +x scripts/runner/onebrain-soak-runner.sh
```

### Cách B — Chỉ tải một file

```bash
mkdir -p ~/onebrain-runner-kit
cd ~/onebrain-runner-kit
curl -fLO \
  https://raw.githubusercontent.com/shpy2001gemi/OneBrain/main/scripts/runner/onebrain-soak-runner.sh
chmod +x onebrain-soak-runner.sh
```

Nên mở và đọc script trước khi chạy:

```bash
less onebrain-soak-runner.sh
```

Các ví dụ còn lại dùng biến sau để hoạt động với cả hai cách:

```bash
RUNNER_KIT=./scripts/runner/onebrain-soak-runner.sh
```

Nếu chỉ tải một file:

```bash
RUNNER_KIT=./onebrain-soak-runner.sh
```

## 5. Kiểm tra và dependency

Chạy preflight:

```bash
"$RUNNER_KIT" doctor
```

Script kiểm tra:

- Linux x64;
- compiler/linker và công cụ build;
- RAM/disk;
- NTP;
- kết nối HTTPS tới GitHub;
- không yêu cầu inbound port.

Nếu thiếu dependency trên Ubuntu/Debian hoặc Rocky/Alma/RHEL/CentOS Stream 8+:

```bash
"$RUNNER_KIT" deps
"$RUNNER_KIT" doctor
```

Lệnh `deps` tự nhận diện `apt`, `dnf` hoặc `yum`. Có thể chạy lệnh này bằng
`root`, hoặc bằng user có `sudo`; GitHub runner vẫn nằm portable dưới:

```text
~/.local/share/onebrain-actions-runner
```

Không có systemd service được tạo.

Nếu đang đăng nhập bằng `root`, chỉ dùng `root` để chạy `deps` và tạo user riêng.
Không chạy `setup`, `setup-run`, `run` hoặc `start` bằng `root`:

```bash
./onebrain-soak-runner.sh deps
useradd --create-home --shell /bin/bash onebrain
install -d -o onebrain -g onebrain /home/onebrain/onebrain-runner-kit
install -m 0755 -o onebrain -g onebrain \
  ./onebrain-soak-runner.sh \
  /home/onebrain/onebrain-runner-kit/onebrain-soak-runner.sh
su - onebrain
cd ~/onebrain-runner-kit
./onebrain-soak-runner.sh doctor
```

## 6. Lấy registration token

Trên GitHub:

1. Mở repository `shpy2001gemi/OneBrain`.
2. Chọn **Settings → Actions → Runners**.
3. Chọn **New self-hosted runner**.
4. Chọn **Linux** và **x64**.
5. Trong command `./config.sh --url ... --token ...`, chỉ copy giá trị sau
   `--token`.

Token đăng ký có hiệu lực ngắn. Không gửi token qua chat, không commit và không
ghi vào file.

## 7. Cách chạy đơn giản nhất

Chạy script không có argument để mở menu:

```bash
"$RUNNER_KIT"
```

Chọn:

```text
2) First-time ephemeral setup and run
```

Hoặc chạy thẳng:

```bash
"$RUNNER_KIT" setup-run
```

Script sẽ:

1. chạy doctor;
2. lấy metadata bản `actions/runner` mới nhất từ GitHub;
3. tải Linux x64 archive;
4. xác minh SHA-256 từ GitHub release metadata;
5. đăng ký runner với nhãn `onebrain-soak`;
6. chạy foreground;
7. nhận đúng một job;
8. tự deregister và thoát sau khi job kết thúc.

Giữ terminal/SSH session mở. Có thể dùng `tmux` để tránh mất runner khi SSH
disconnect:

```bash
tmux new -s onebrain-soak
"$RUNNER_KIT" setup-run
```

Detach bằng `Ctrl+B`, rồi `D`. Quay lại:

```bash
tmux attach -t onebrain-soak
```

`tmux` là tùy chọn; nếu chưa có thì dùng background mode ở phần 9.

## 8. Khởi động workflow soak

Trên GitHub:

1. Mở **Actions**.
2. Chọn **vNext soak and release gate**.
3. Chọn **Run workflow**.
4. Chọn branch `main`.
5. Chọn profile:
   - `nightly-24h`; hoặc
   - `pre-release-72h`.
6. Chọn **Run workflow**.

Có thể queue workflow trước rồi bật runner, hoặc bật runner trước rồi dispatch.
Nếu không có runner online phù hợp, GitHub giữ job ở trạng thái queued tối đa
24 giờ.

Khi hoàn tất, tải JSON artifact từ trang workflow run:

- `vnext-soak-nightly-24h-...`; hoặc
- `vnext-soak-pre-release-72h-...`.

`smoke` dùng `ubuntu-latest`, không cần self-hosted runner.

## 9. Chạy nền và bật/tắt thủ công

Nếu runner đã đăng ký:

```bash
"$RUNNER_KIT" start
"$RUNNER_KIT" status
"$RUNNER_KIT" logs
```

Tắt:

```bash
"$RUNNER_KIT" stop
```

Script yêu cầu gõ `STOP`, vì dừng trong lúc soak sẽ làm job thất bại.

Ephemeral runner chạy nền:

```bash
"$RUNNER_KIT" setup
"$RUNNER_KIT" start
```

Sau một job, runner tự deregister và process thoát.

## 10. Persistent mode

Chỉ dùng nếu muốn máy tự nhận `nightly-24h` theo lịch hằng ngày:

```bash
"$RUNNER_KIT" setup --persistent
"$RUNNER_KIT" start
```

Persistent mode không cài service. Sau reboot phải chạy lại `start`. Tắt bằng
`stop`.

Với repo public, ephemeral mode an toàn hơn và là mặc định.

## 11. Gỡ cài đặt

### Deregister trên GitHub

Mở:

**Settings → Actions → Runners → tên runner → Remove**

Copy removal token, rồi chạy:

```bash
"$RUNNER_KIT" remove
```

Token được nhập ẩn. Script không lưu token.

### Xóa toàn bộ file local

Sau khi runner không còn trên GitHub:

```bash
"$RUNNER_KIT" purge
```

Gõ `PURGE` để xác nhận. Thao tác này xóa runner binary, `_work`, build cache,
log và dữ liệu soak local trong runner home.

Gỡ cả registration và file local:

```bash
"$RUNNER_KIT" uninstall
```

Ephemeral runner đã hoàn thành job thường tự deregister nên không còn trang để
lấy removal token. Khi đó xác nhận runner đã biến mất tại **Settings → Actions
→ Runners**, rồi chỉ chạy `purge`.

## 12. Xử lý sự cố

Kiểm tra:

```bash
"$RUNNER_KIT" doctor
"$RUNNER_KIT" status
"$RUNNER_KIT" logs
```

### Job luôn queued

Kiểm tra runner có đủ nhãn:

```text
self-hosted
linux
x64
onebrain-soak
```

Kiểm tra workflow được dispatch từ `main`, runner đang online và không bận job
khác.

### Runner download bị lỗi

Kiểm tra outbound 443, DNS và domain allowlist. Nếu GitHub API báo rate limit,
có thể tạm đặt `GITHUB_TOKEN` chỉ cho lần tải metadata; không lưu token vào
script.

### Build thiếu RAM hoặc disk

Chạy `doctor`. Khuyến nghị tối thiểu 8 GiB RAM và 50 GiB SSD. Xóa runner cache
sau khi đã tải artifact:

```bash
"$RUNNER_KIT" purge
```

### SSH bị ngắt

Dùng `tmux` hoặc `start`. Không đóng foreground process trong khi job 24/72 giờ
đang chạy.

### Server reboot giữa soak

Workflow sẽ thất bại và không đủ duration evidence. Khởi động runner, dispatch
lại từ đầu và không chỉnh sửa JSON report để giả lập thời gian đã mất.
