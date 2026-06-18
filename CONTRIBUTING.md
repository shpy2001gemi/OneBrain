# 🤝 Hướng dẫn Đóng góp (Contributing Guide)

Cảm ơn bạn đã quan tâm đến OneBrain! Mọi đóng góp đều được trân trọng — từ sửa lỗi chính tả đến đề xuất kiến trúc hệ thống.

> *"OneBrain tin rằng không đóng góp nào là nhỏ bé — giống như không tri thức nào là không có giá trị."*

---

## 📋 Mục lục

- [Cách đóng góp](#cách-đóng-góp)
- [Quy trình làm việc](#quy-trình-làm-việc)
- [Quy ước đặt tên](#quy-ước-đặt-tên)
- [Commit Messages](#commit-messages)
- [Pull Request](#pull-request)
- [Báo cáo lỗi](#báo-cáo-lỗi)
- [Đề xuất tính năng](#đề-xuất-tính-năng)
- [Cộng đồng](#cộng-đồng)

---

## Cách đóng góp

### 🌟 Dành cho người mới

Nếu bạn chưa quen với open source, đừng lo! Đây là những cách bạn có thể bắt đầu:

1. **⭐ Star** repository này để ủng hộ dự án
2. **📖 Đọc** tài liệu và góp ý cải thiện
3. **🐛 Báo lỗi** nếu bạn tìm thấy vấn đề
4. **💡 Đề xuất ý tưởng** cho tính năng mới
5. **🌐 Dịch** tài liệu sang ngôn ngữ khác
6. **📝 Cải thiện** tài liệu hiện có

### 🔧 Dành cho lập trình viên

1. **Fork** repository
2. **Clone** về máy local
3. Tạo **branch** mới cho tính năng/sửa lỗi
4. **Code** và viết test
5. Gửi **Pull Request**

---

## Quy trình làm việc

### 1. Fork & Clone

```bash
# Fork repo trên GitHub, sau đó:
git clone https://github.com/<your-username>/OneBrain.git
cd OneBrain
git remote add upstream https://github.com/onebrain-project/OneBrain.git
```

### 2. Tạo Branch

```bash
# Cập nhật main branch
git checkout main
git pull upstream main

# Tạo branch mới
git checkout -b feature/ten-tinh-nang
# hoặc
git checkout -b fix/ten-loi
# hoặc
git checkout -b docs/ten-tai-lieu
```

### 3. Phát triển

- Viết code sạch, có comment
- Tuân theo coding style của dự án
- Viết tests cho code mới
- Cập nhật tài liệu nếu cần

### 4. Commit & Push

```bash
git add .
git commit -m "feat: mô tả ngắn gọn thay đổi"
git push origin feature/ten-tinh-nang
```

### 5. Tạo Pull Request

- Vào GitHub và tạo Pull Request từ branch của bạn
- Điền đầy đủ thông tin theo template
- Chờ review từ maintainers

---

## Quy ước đặt tên

### Branches

| Prefix | Mục đích | Ví dụ |
|---|---|---|
| `feature/` | Tính năng mới | `feature/knowledge-graph-api` |
| `fix/` | Sửa lỗi | `fix/voting-calculation` |
| `docs/` | Tài liệu | `docs/api-reference` |
| `refactor/` | Tái cấu trúc code | `refactor/consensus-engine` |
| `test/` | Thêm/sửa tests | `test/pok-protocol` |
| `chore/` | Việc bảo trì | `chore/update-dependencies` |

---

## Commit Messages

Sử dụng [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

### Types

| Type | Mô tả |
|---|---|
| `feat` | Tính năng mới |
| `fix` | Sửa lỗi |
| `docs` | Thay đổi tài liệu |
| `style` | Format, thiếu dấu chấm phẩy, etc. |
| `refactor` | Tái cấu trúc code |
| `test` | Thêm hoặc sửa tests |
| `chore` | Bảo trì, cập nhật dependencies |
| `perf` | Cải thiện hiệu suất |

### Ví dụ

```
feat(knowledge-graph): add knowledge unit linking algorithm
fix(voting): correct weighted vote calculation for high-rep users
docs(readme): add BCI integration use case
```

---

## Pull Request

Khi tạo PR, vui lòng:

1. **Mô tả rõ ràng** thay đổi của bạn
2. **Liên kết issue** liên quan (nếu có): `Fixes #123`
3. **Screenshots/recordings** cho thay đổi UI
4. **Checklist:**
   - [ ] Code tuân theo coding style của dự án
   - [ ] Đã tự review code của mình
   - [ ] Đã thêm comment cho code phức tạp
   - [ ] Đã cập nhật tài liệu
   - [ ] Thay đổi không tạo ra warnings mới
   - [ ] Đã thêm tests
   - [ ] Tất cả tests pass

---

## Báo cáo lỗi

Khi báo cáo lỗi, vui lòng bao gồm:

1. **Tiêu đề** ngắn gọn, rõ ràng
2. **Các bước tái hiện** lỗi
3. **Kết quả mong đợi** vs. **kết quả thực tế**
4. **Môi trường**: OS, browser, phiên bản, etc.
5. **Screenshots** nếu có thể

---

## Đề xuất tính năng

Khi đề xuất tính năng, vui lòng mô tả:

1. **Vấn đề** mà tính năng giải quyết
2. **Giải pháp** bạn đề xuất
3. **Alternatives** — các giải pháp thay thế đã cân nhắc
4. **Ngữ cảnh** — bối cảnh bổ sung

---

## Cộng đồng

### 💬 Liên hệ

- **Email:** shpy2001@gmail.com
- **Discussions:** GitHub Discussions (coming soon)
- **Discord:** Coming soon

### 🌍 Ngôn ngữ

- Tài liệu chính: **Tiếng Việt** và **Tiếng Anh**
- Code comments: **Tiếng Anh**
- Issues & PRs: Tiếng Việt hoặc Tiếng Anh đều được

### 🏆 Ghi nhận đóng góp

Tất cả contributors sẽ được ghi nhận trong file [CONTRIBUTORS.md](CONTRIBUTORS.md). Đúng tinh thần OneBrain — mọi đóng góp đều có giá trị!

---

*Cảm ơn bạn đã giúp xây dựng OneBrain — bộ não chung của nhân loại!* 🧠
