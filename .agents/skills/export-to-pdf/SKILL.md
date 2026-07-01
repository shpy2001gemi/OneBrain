---
name: export-to-pdf
description: Xuất file Markdown (.md) sang PDF chuyên nghiệp với styling đẹp (Inter font, gradient table headers, colored sections). Tự động render Mermaid diagrams thành hình ảnh PNG. Dùng khi cần file PDF để gửi khách hàng hoặc stakeholders.
---

# Skill: Export Markdown to Styled PDF (with Mermaid Rendering)

## Khi nào dùng skill này

- Khi user nói: "Xuất file này sang PDF", "Export sang PDF", "Chuyển sang pdf"
- Khi user yêu cầu xuất file .md có chứa mermaid diagrams sang PDF
- Khi user nói: "xuất pdf kèm mermaid", "render mermaid ra PDF"

## Yêu cầu hệ thống (Prerequisites)

| Tool | Kiểm tra | Cài đặt nếu thiếu |
|---|---|---|
| **Node.js** | `node --version` | Tải từ nodejs.org |
| **marked** | npm package | `npm install marked --no-save` (trong workspace) |
| **puppeteer** | npm package | `npm install puppeteer --no-save` (trong workspace) |
| **@mermaid-js/mermaid-cli** | Tự cài qua `npx -y` (pin version 10.9.1) | Không cần cài trước |

> **LƯU Ý:** Script cần `marked` và `puppeteer` trong `node_modules` của workspace.
> Nếu chưa có, chạy: `npm install marked puppeteer --no-save` trong workspace root.

## Quy trình bắt buộc (MANDATORY)

### Bước 1: Đảm bảo dependencies

```
# Set NODE_PATH nếu cần (Windows PowerShell)
$env:NODE_PATH = "<workspace>/node_modules"
```

### Bước 2: Chạy script bundled

```
node <skill-path>/scripts/export_pdf.js "<input.md>" "<output.pdf>"
```

Ví dụ:
```
node "C:\path\to\.agent\skills\export-to-pdf\scripts\export_pdf.js" "input.md" "output.pdf"
```

**Nếu `marked` không tìm được**, set `NODE_PATH`:
```powershell
$env:NODE_PATH = "c:\path\to\workspace\node_modules"; node <skill-path>/scripts/export_pdf.js "input.md" "output.pdf"
```

**Script tự động xử lý toàn bộ pipeline:**
1. Tìm tất cả mermaid code blocks
2. Render mỗi block thành PNG 2000px width (via mermaid-cli@10.9.1) → **save to `_export/`**
3. Convert MD → HTML bằng **marked** (GitHub Flavored Markdown)
4. Inject **premium CSS** (Inter font, gradient table headers, colored h2/h3, dark code blocks)
5. Embed mermaid PNGs + local images dưới dạng base64 data URI → **copy to `_export/`**
6. Puppeteer in HTML → PDF A4 với header (tên tài liệu) + footer (số trang)

### Bước 3: Thông báo kết quả

- Đường dẫn file .pdf
- Kích thước file
- Số lượng mermaid diagrams đã render (nếu có)
- Đường dẫn `_export/` folder

## CSS Styling Features

| Feature | Chi tiết |
|---|---|
| **Font** | Inter (Google Fonts), fallback Segoe UI |
| **H1** | 24px, border-bottom xanh |
| **H2** | 17px, nền gradient xanh nhạt, border-left xanh đậm |
| **H3** | 13px, nền xám nhạt, border-left xám |
| **Tables** | Gradient navy→blue header, zebra rows, rounded corners, shadow |
| **Code blocks** | Dark background (#1e293b), monospace font |
| **Blockquotes** | Nền vàng nhạt, border-left vàng |
| **HR** | Gradient xanh → tím → xanh |
| **Header/Footer** | Tên tài liệu (header) + Trang X/Y (footer) |
| **Images** | Auto-scale: `max-height:700px`, `object-fit:contain`, `page-break-inside:avoid` |

## Output

- File .pdf lưu theo đường dẫn output chỉ định
- **`_export/` folder** cùng thư mục với output chứa tất cả images:
  - `<output-name>_diagram_001.png` — mermaid diagrams rendered
  - `<output-name>_<filename>.png` — local mockup/workflow images
- Images được embed base64 trong PDF **VÀ** save riêng trong `_export/`

## Lưu ý

- **PHẢI dùng mermaid-cli@10.9.1** — phiên bản 11.x+ có breaking changes
- Puppeteer cần Chrome headless (~100MB lần đầu, cache sau đó)
- Nếu mermaid render fail → giữ nguyên code block, tiếp tục
- Script hỗ trợ cả Windows và macOS/Linux
- **Hình ảnh tự scale** — mockup/workflow images được giới hạn `max-height: 700px` để không vượt quá 1 trang A4

## So sánh v1 vs v2 vs v3

| | v1 (Pandoc HTML) | v2 (marked + CSS styled) | **v3 (auto-scale images)** |
|---|---|---|---|
| MD → HTML | Pandoc | **marked** (GFM) | marked (GFM) |
| Styling | CSS inject basic | **Premium CSS** (gradients) | Premium CSS |
| Font | Segoe UI | **Inter** (Google Fonts) | Inter |
| Table header | Solid blue | **Gradient navy→blue** | Gradient |
| Code blocks | Light gray | **Dark theme (#1e293b)** | Dark theme |
| Headings | Plain colored text | **Colored blocks** | Colored blocks |
| **Images** | No constraints | `max-width:100%` only | **✅ max-height:700px + page-break-inside:avoid** |
