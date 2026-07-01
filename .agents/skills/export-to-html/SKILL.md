---
name: export-to-html
description: Xuất file Markdown (.md) sang HTML đẹp với premium CSS (Inter font, gradient table headers, zebra rows, colored sections). Tự động render Mermaid diagrams thành hình ảnh. Không cần Pandoc, Puppeteer hay dependencies nặng — chỉ cần Node.js + marked.
---

# Skill: Export Markdown to Styled HTML (with Mermaid Rendering)

## Khi nào dùng skill này

- Khi user nói: "Xuất file này sang HTML", "Export sang HTML", "Chuyển sang html"
- Khi user cần preview đẹp trên browser
- Khi user nói: "xuất html", "render html"
- Khi user muốn file có thể mở trên mọi máy, mọi browser

## Ưu điểm so với PDF/DOCX

- **Nhẹ**: Chỉ cần `marked` (1 dependency), không cần Puppeteer hay Pandoc
- **Nhanh**: Export trong < 2 giây
- **Đẹp**: Premium CSS với Inter font, gradient headers, hover effects
- **Portable**: Mở được trên mọi browser, mọi máy
- **Print-friendly**: Có `@media print` CSS, print trực tiếp từ browser → PDF

## Yêu cầu hệ thống (Prerequisites)

| Tool | Kiểm tra | Cài đặt nếu thiếu |
|---|---|---|
| **Node.js** | `node --version` | Tải từ nodejs.org |
| **marked** | npm package | `npm install marked` (trong workspace) |
| **@mermaid-js/mermaid-cli** | Tự cài qua `npx -y` (pin version 10.9.1) | Chỉ cần nếu MD có mermaid blocks |

> **LƯU Ý:** Script cần `marked` trong `node_modules` của workspace.
> Nếu chưa có, chạy: `npm install marked` trong workspace root.

## Quy trình bắt buộc (MANDATORY)

### Bước 1: Đảm bảo dependencies

```powershell
# Set NODE_PATH nếu cần (Windows PowerShell)
$env:NODE_PATH = "<workspace>/node_modules"
```

### Bước 2: Chạy script bundled

```powershell
node <skill-path>/scripts/export_html.js "<input.md>" "<output.html>"
```

Ví dụ:
```powershell
node "c:\path\.agent\skills\export-to-html\scripts\export_html.js" "input.md" "output.html"
```

**Nếu `marked` không tìm được**, set `NODE_PATH`:
```powershell
$env:NODE_PATH = "c:\path\to\workspace\node_modules"; node <skill-path>/scripts/export_html.js "input.md" "output.html"
```

### Bước 3: Output

- File `.html` cùng thư mục với input (hoặc path chỉ định)
- Mở bằng browser để xem
- In trực tiếp từ browser (Ctrl+P) để ra PDF nếu cần

## Tính năng Styling

| Feature | Chi tiết |
|---|---|
| **Font** | Inter (Google Fonts CDN) + fallback system fonts |
| **Table headers** | Gradient navy → blue, white text |
| **Table rows** | Zebra stripes (even/odd), hover highlight |
| **Headings** | H1: border-bottom blue, H2: gradient background + left border, H3: grey left border |
| **Blockquotes** | Left border amber, yellow background |
| **Code blocks** | Dark background (#1e293b), monospace font |
| **Inline code** | Blue background, purple text |
| **HR** | Blue accent line |
| **Responsive** | Max-width 1100px, auto margins |
| **Print** | `@media print` removes background, full width |

## Mermaid Rendering

- Script tự detect các code blocks ` ```mermaid `
- Render thành PNG via `@mermaid-js/mermaid-cli@10.9.1`
- Embed vào HTML dưới dạng `<img>` (file path)
- Nếu không có mermaid blocks → bỏ qua, không lỗi

## Lưu ý

- Output là **self-contained HTML** (CSS inline trong `<style>`, chỉ cần Google Fonts CDN)
- Nếu muốn hoàn toàn offline, có thể tải font Inter local
- GitHub-style alerts (`> [!NOTE]`, `> [!WARNING]`, ...) được tự động convert thành blockquote có emoji
