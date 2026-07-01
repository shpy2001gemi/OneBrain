---
name: export-to-docx
description: Xuất file Markdown (.md) sang Microsoft Word (.docx) chuyên nghiệp với styling đẹp (colored headers, styled tables). Tự động render Mermaid diagrams thành hình ảnh PNG và lưu vào folder có cấu trúc. Dùng khi cần file Word để gửi khách hàng hoặc PO.
---

# Skill: Export Markdown to Styled DOCX (with Mermaid Rendering)

## Khi nào dùng skill này

- Khi user nói: "Xuất file này sang DOCX", "Export sang Word", "Chuyển requirement thành docx"
- Khi user yêu cầu xuất file .md có chứa mermaid diagrams
- Khi user nói: "xuất docx kèm mermaid", "render mermaid ra hình"

## Yêu cầu hệ thống (Prerequisites)

| Tool | Kiểm tra | Cài đặt nếu thiếu |
|---|---|---|
| **Node.js** | `node --version` | Tải từ nodejs.org |
| **marked** | npm package | `npm install marked --no-save` |
| **@turbodocx/html-to-docx** | npm package | `npm install @turbodocx/html-to-docx --no-save` |
| **@mermaid-js/mermaid-cli** | Tự cài qua `npx -y` (pin 10.9.1) | Không cần cài trước |

> **LƯU Ý:** Script cần `marked` và `@turbodocx/html-to-docx` trong `node_modules` của workspace.
> Nếu chưa có, chạy: `npm install marked @turbodocx/html-to-docx --no-save` trong workspace root.
> **Pandoc không cần thiết** — turbodocx xử lý HTML → DOCX trực tiếp trong Node.js.

## Quy trình bắt buộc (MANDATORY)

### Bước 0: Kiểm tra & Tạo Mockup Images (NẾU CÓ)

> ⚠️ **BẮT BUỘC:** Trước khi export, PHẢI quét file MD tìm **ASCII wireframe mockups** (code blocks chứa ký tự `┌`, `│`, `└`, `┘`). Nếu có:

1. **Quét file:** Tìm các code blocks có box-drawing characters (wireframes).
2. **Generate images:** Dùng tool `generate_image` từ skill `mockup-image-generator` để tạo hình mockup thật từ mô tả ASCII art.
   - Prompt phải mô tả chi tiết UI: layout, màu sắc, font, nội dung text.
   - Style: Clean modern design, Inter font, blue primary #2563eb.
3. **Lưu images:** Copy vào folder `_mockups/` cùng thư mục với file MD.
   - Naming: `mockup_<tên_mô_tả>.png`
4. **Embed trong MD:** Thêm `![Mockup: <mô tả>](_mockups/<filename>.png)` ngay SAU code block ASCII art.
5. **DOCX output:** Khi export, script sẽ tự embed hình base64 vào DOCX.

**Ví dụ:**
```markdown
### 5.2. NPP Quản Lý Đơn
```
┌──────────────────────┐
│  📦 ĐƠN HÀNG        │
└──────────────────────┘
```

![Mockup: NPP Quản Lý Đơn](_mockups/mockup_npp_orders.png)
```

> 🔴 **Nếu KHÔNG có wireframe ASCII:** Bỏ qua bước này, nhảy thẳng Bước 1.

### Bước 1: Chạy script bundled

```powershell
$env:NODE_PATH = "<workspace>/node_modules"; node <skill-path>/scripts/export_docx_mermaid.js "<input.md>" "<output.docx>"
```

Ví dụ:
```powershell
$env:NODE_PATH = "c:\path\to\workspace\node_modules"; node "c:\path\to\.agent\skills\export-to-docx\scripts\export_docx_mermaid.js" "input.md" "output.docx"
```

> ⚠️ **QUAN TRỌNG:** Luôn set `NODE_PATH` để script tìm được npm packages.

**Script tự động xử lý toàn bộ pipeline (v10):**
1. Tìm tất cả mermaid code blocks → render thành PNG (1200px, scale 2x)
2. Clean GitHub-style alerts (`[!NOTE]`, `[!WARNING]`, etc.)
3. Convert MD → HTML bằng **marked** (GFM)
4. Embed local images dưới dạng **base64 data URIs**
5. Apply **premium inline styles** với table wrappers (turbodocx-specific)
6. **@turbodocx/html-to-docx** → DOCX (via temp file → copy to output)
7. **Save images → `_export/` folder** cùng thư mục output

### Bước 2: Thông báo kết quả

- Đường dẫn file .docx
- Kích thước file
- Số lượng mermaid diagrams đã render (nếu có)
- Số lượng images embedded
- Đường dẫn `_export/` folder

## Output

- File .docx lưu theo đường dẫn output chỉ định
- **`_export/` folder** cùng thư mục với output chứa tất cả images:
  - `<output-name>_diagram_001.png` — mermaid diagrams rendered
  - `<output-name>_<filename>.png` — local mockup/workflow images
- Images được embed base64 trong DOCX **VÀ** save riêng trong `_export/`

### Bước 2: Thông báo kết quả

- Đường dẫn file .docx
- Kích thước file
- Số lượng mermaid diagrams đã render (nếu có)
- Số lượng images embedded
- **Đường dẫn assets folder** (chứa PNG/images đã lưu)

## Styling Features (v8 — PDF-matching)

turbodocx chỉ render `background-color` trên `<td>` elements. Vì vậy H2, H3, blockquotes, và code blocks đều dùng **table wrappers** để có nền màu.

| Feature | Kỹ thuật | Chi tiết |
|---|---|---|
| **H1** | `<h1>` trực tiếp | 22pt, bold, `border-bottom: 3px solid #2563eb` |
| **H2** | `<table><td>` wrapper | 16pt, bold, nền xanh nhạt `#dbeafe`, border-left `#2563eb` |
| **H3** | `<table><td>` wrapper | 13pt, bold, nền xám `#f1f5f9`, border-left `#64748b` |
| **Tables** | `<th>` styling | Dark navy header `#1e3a5f`, white text, zebra rows |
| **Code blocks** | `<table>` 2 rows | Dark header "CODE" `#1e293b` + light body `#f8fafc`, Consolas 9pt |
| **Blockquotes** | `<table><td>` wrapper | Nền vàng `#fef3c7`, border-left `#f59e0b`, text `#92400e` |
| **Inline code** | `<code>` styling | Consolas, nền tím nhạt `#e0e7ff`, text `#3730a3` |
| **Images** | Base64 embedded + auto-scale | Local images → data URI, scale to fit page (max 610×800px) |

## Code Block Format

Mỗi dòng code = 1 `<p style="margin:0">` bên trong 1 `<td>`. Điều này:
- ✅ Xuống dòng đúng (không bị dính 1 nùi)
- ✅ Giữ indentation (spaces → `&nbsp;`, tabs → 4 spaces)
- ✅ Copy ra là text (không phải table rows)
- ✅ Có header bar tối "CODE" cho đẹp

## Lưu ý quan trọng

### File bị lock
> ⚠️ **Phải đóng file .docx trong Word** trước khi export đè lên. Nếu Word lock file → `copyFileSync` sẽ fail. Giải pháp: dùng tên file khác (vd: `_v2.docx`).

### Path có dấu cách
Script ghi DOCX vào **temp directory** trước rồi `copyFileSync` sang output. Điều này tránh lỗi với đường dẫn có dấu cách (OneDrive, "Antigravity iMES", etc.).

### Thứ tự regex
Structural replacements (H2→table, H3→table, blockquote→table, code→table) **PHẢI chạy TRƯỚC** generic tag replacements (`<table>`, `<td>`, `<p>`). Nếu đảo thứ tự → style conflict.

### Packages
- ❌ `html-to-docx` (cũ) — broken, crash với base64 images
- ✅ `@turbodocx/html-to-docx` — fork maintained, hoạt động tốt (Feb 2026)

## Pipeline Evolution

| Version | Engine | Styling | Kết quả |
|---|---|---|---|
| v1 | Pandoc direct | Lua filter | Basic, code blocks only |
| v2 | Pandoc HTML | `<style>` CSS block | Professional nhưng Pandoc bỏ CSS |
| v6 | Pandoc HTML | Inline styles | Ổn nhưng code blocks không format |
| v7 | turbodocx | Inline `<pre>` | Images OK, code break không đúng |
| v8 | turbodocx | Table wrappers | ✅ PDF-matching: colors, images, code |
| v9 | turbodocx | Table wrappers + assets | ✅ v8 + lưu hình vào folder phân cấp |
| **v10** | **turbodocx** | **Auto-scale images** | **✅ v9 + hình auto-scale vừa 1 trang (610×800px)** |
