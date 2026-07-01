/**
 * Export Markdown to Styled PDF with Mermaid Rendering
 * 
 * Usage: node export_pdf.js <input.md> <output.pdf>
 * 
 * Pipeline:
 *   1. Find mermaid code blocks
 *   2. Render each to PNG via mermaid-cli@10.9.1 (2000px width) → save to _export/
 *   3. Convert MD → HTML via marked (GitHub-flavored)
 *   4. Inject premium CSS (Inter font, gradient table headers, colored sections)
 *   5. Embed mermaid PNGs + local images as base64 data URIs → copy to _export/
 *   6. Puppeteer → PDF A4 with header/footer
 * 
 * Images saved to: _export/ folder next to output PDF
 * Requirements: Node.js, marked, puppeteer
 * Auto-installs: @mermaid-js/mermaid-cli@10.9.1
 */

const { execSync } = require('child_process');
const fs = require('fs');
const path = require('path');
const os = require('os');

// --- Args ---
const inputMd = process.argv[2];
const outputPdf = process.argv[3];
if (!inputMd || !outputPdf) {
  console.error('Usage: node export_pdf.js <input.md> <output.pdf>');
  process.exit(1);
}

console.log(`📄 Export Markdown → Styled PDF`);
console.log(`   Input:  ${inputMd}`);
console.log(`   Output: ${outputPdf}`);

const content = fs.readFileSync(inputMd, 'utf-8').replace(/\r\n/g, '\n');
const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'md2pdf_'));
const docTitle = path.basename(inputMd, path.extname(inputMd)).replace(/[-_]/g, ' ');

// Create _export folder next to output file
const outputBaseName = path.basename(outputPdf, path.extname(outputPdf));
const exportDir = path.join(path.dirname(path.resolve(outputPdf)), '_export');
fs.mkdirSync(exportDir, { recursive: true });
console.log(`   Images: ${exportDir}`);

// ==================================================
// Step 1: Find & render mermaid diagrams
// ==================================================
const MERMAID_CLI_VERSION = '10.9.1';
const MERMAID_WIDTH = 4000;
const MERMAID_SCALE = 2;

const mermaidRegex = /```mermaid\n([\s\S]*?)```/g;
let match;
const diagrams = [];
while ((match = mermaidRegex.exec(content)) !== null) {
  diagrams.push({ code: match[1], fullMatch: match[0] });
}
console.log(`🔍 Found ${diagrams.length} mermaid diagram(s)`);

let processedContent = content;
const imageFiles = [];

for (let i = 0; i < diagrams.length; i++) {
  const d = diagrams[i];
  const mmdFile = path.join(tempDir, `d${i}.mmd`);
  const pngFile = path.join(tempDir, `d${i}.png`);
  fs.writeFileSync(mmdFile, Buffer.from(d.code.trim(), 'utf8'));
  console.log(`🎨 Rendering diagram ${i + 1}/${diagrams.length}...`);

  let ok = false;
  const commands = [
    `npx -y @mermaid-js/mermaid-cli@${MERMAID_CLI_VERSION} mmdc -i "${mmdFile}" -o "${pngFile}" -b white -w ${MERMAID_WIDTH}`,
    `npx -y @mermaid-js/mermaid-cli@${MERMAID_CLI_VERSION} mmdc -i "${mmdFile}" -o "${pngFile}" -b white`
  ];

  for (const cmd of commands) {
    if (ok) break;
    try {
      execSync(cmd, { stdio: 'pipe', timeout: 120000 });
      if (fs.existsSync(pngFile) && fs.statSync(pngFile).size > 200) {
        const sz = (fs.statSync(pngFile).size / 1024).toFixed(0);
        console.log(`  ✅ d${i}.png (${sz} KB)`);
        const b64 = fs.readFileSync(pngFile).toString('base64');
        imageFiles.push({ index: i, dataUri: `data:image/png;base64,${b64}` });
        processedContent = processedContent.replace(d.fullMatch, `![diagram_${i}](MERMAID_${i})`);
        // Save to _export folder
        const assetName = `${outputBaseName}_diagram_${String(i + 1).padStart(3, '0')}.png`;
        const assetPath = path.join(exportDir, assetName);
        fs.copyFileSync(pngFile, assetPath);
        console.log(`  ✅ Saved → ${path.relative(process.cwd(), assetPath)}`);
        ok = true;
      }
    } catch (e) { /* try next */ }
  }
  if (!ok) console.log(`  ❌ Failed diagram ${i}, keeping code block`);
}

// ==================================================
// Step 2: Clean up GitHub-style alerts
// ==================================================
processedContent = processedContent.replace(/^> \[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\]\s*$/gm, (m, type) => {
  const emoji = { NOTE: 'ℹ️', TIP: '💡', IMPORTANT: '❗', WARNING: '⚠️', CAUTION: '🔴' }[type] || '📌';
  return `> **${emoji} ${type}:**`;
});

// ==================================================
// Step 3: Convert MD → HTML using marked
// ==================================================
console.log(`📝 Converting to styled HTML...`);

let markedModule;
try {
  markedModule = require('marked');
} catch {
  // Walk up from script dir to find node_modules
  let dir = __dirname;
  let found = false;
  for (let i = 0; i < 10; i++) {
    const modPath = path.join(dir, 'node_modules', 'marked');
    if (fs.existsSync(modPath)) {
      markedModule = require(modPath);
      found = true;
      break;
    }
    const parent = path.dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  if (!found && process.env.NODE_PATH) {
    const envPath = path.join(process.env.NODE_PATH, 'marked');
    if (fs.existsSync(envPath)) markedModule = require(envPath);
  }
  if (!markedModule) {
    console.error('❌ marked not found. Run: npm install marked --no-save');
    process.exit(1);
  }
}
const { marked } = markedModule;

let htmlBody = marked.parse(processedContent);

// Embed local images as base64 data URIs (fixes puppeteer setContent missing images)
const inputDir = path.dirname(path.resolve(inputMd));
htmlBody = htmlBody.replace(/src="([^"]+)"/g, (fullMatch, srcPath) => {
  // Skip data URIs (already embedded mermaid), http(s) URLs
  if (srcPath.startsWith('data:') || srcPath.startsWith('http')) return fullMatch;
  // Resolve relative path from input MD location
  const absPath = path.resolve(inputDir, decodeURIComponent(srcPath));
  if (fs.existsSync(absPath)) {
    const ext = path.extname(absPath).toLowerCase();
    const mimeMap = { '.png': 'image/png', '.jpg': 'image/jpeg', '.jpeg': 'image/jpeg', '.gif': 'image/gif', '.svg': 'image/svg+xml', '.webp': 'image/webp' };
    const mime = mimeMap[ext] || 'image/png';
    const b64 = fs.readFileSync(absPath).toString('base64');
    console.log(`  📷 Embedded: ${path.basename(absPath)} (${(fs.statSync(absPath).size / 1024).toFixed(0)} KB)`);
    // Save to _export folder
    const destName = `${outputBaseName}_${path.basename(absPath)}`;
    const destPath = path.join(exportDir, destName);
    fs.copyFileSync(absPath, destPath);
    console.log(`  ✅ Saved → ${path.relative(process.cwd(), destPath)}`);
    return `src="data:${mime};base64,${b64}"`;
  } else {
    console.log(`  ⚠️ Image not found: ${absPath}`);
    return fullMatch;
  }
});

// Replace mermaid image placeholders with base64 data URIs
for (const img of imageFiles) {
  htmlBody = htmlBody.replace(
    new RegExp(`src="MERMAID_${img.index}"`, 'g'),
    `src="${img.dataUri}"`
  );
}

// ==================================================
// Step 4: Build full HTML with premium CSS
// ==================================================
const CSS = `
  @import url('https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&display=swap');

  * { margin: 0; padding: 0; box-sizing: border-box; }

  body {
    font-family: 'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
    font-size: 11px;
    line-height: 1.6;
    color: #1a1a2e;
    padding: 40px 50px;
    background: #fff;
  }

  h1 {
    font-size: 24px;
    font-weight: 700;
    color: #0f172a;
    border-bottom: 3px solid #2563eb;
    padding-bottom: 10px;
    margin-bottom: 6px;
    margin-top: 30px;
  }
  h1:first-child { margin-top: 0; }

  h2 {
    font-size: 17px;
    font-weight: 700;
    color: #1e40af;
    margin-top: 28px;
    margin-bottom: 6px;
    padding: 6px 12px;
    background: linear-gradient(135deg, #eff6ff, #dbeafe);
    border-left: 4px solid #2563eb;
    border-radius: 0 6px 6px 0;
    page-break-after: avoid;
  }

  h3 {
    font-size: 13px;
    font-weight: 600;
    color: #334155;
    margin-top: 18px;
    margin-bottom: 6px;
    padding: 4px 10px;
    background: #f8fafc;
    border-left: 3px solid #64748b;
    border-radius: 0 4px 4px 0;
    page-break-after: avoid;
  }

  h4 { font-size: 12px; font-weight: 600; color: #475569; margin-top: 14px; margin-bottom: 4px; }

  p { margin: 6px 0; color: #374151; }

  blockquote {
    margin: 8px 0;
    padding: 8px 14px;
    background: #fef3c7;
    border-left: 4px solid #f59e0b;
    border-radius: 0 6px 6px 0;
    color: #92400e;
    font-size: 10.5px;
  }
  blockquote p { margin: 2px 0; color: inherit; }

  strong { font-weight: 600; color: #0f172a; }

  code {
    background: #f1f5f9;
    padding: 1px 5px;
    border-radius: 3px;
    font-family: 'Cascadia Code', 'Fira Code', Consolas, monospace;
    font-size: 10px;
    color: #dc2626;
  }

  pre {
    background: #1e293b;
    color: #e2e8f0;
    padding: 14px 18px;
    border-radius: 8px;
    overflow-x: auto;
    margin: 10px 0;
    font-size: 10px;
    line-height: 1.5;
  }
  pre code { background: none; color: inherit; padding: 0; }

  table {
    width: 100%;
    border-collapse: separate;
    border-spacing: 0;
    margin: 8px 0 14px 0;
    font-size: 10px;
    border-radius: 8px;
    overflow: hidden;
    box-shadow: 0 1px 3px rgba(0,0,0,0.08);
    page-break-inside: auto;
  }

  thead tr {
    background: linear-gradient(135deg, #1e3a5f, #2563eb);
    color: #fff;
  }
  thead th {
    padding: 8px 10px;
    text-align: left;
    font-weight: 600;
    font-size: 10px;
    letter-spacing: 0.02em;
    white-space: nowrap;
  }

  tbody tr { page-break-inside: avoid; }
  tbody tr:nth-child(even) { background: #f8fafc; }
  tbody tr:nth-child(odd) { background: #fff; }

  tbody td {
    padding: 6px 10px;
    border-bottom: 1px solid #e2e8f0;
    color: #334155;
    vertical-align: top;
  }
  tbody td:first-child {
    font-weight: 600;
    color: #1e40af;
    text-align: left;
    white-space: nowrap;
  }

  hr {
    border: none;
    height: 2px;
    background: linear-gradient(to right, #2563eb, #7c3aed, #2563eb);
    margin: 24px 0;
    border-radius: 2px;
  }

  ul, ol { margin: 6px 0 6px 20px; color: #374151; }
  li { margin: 3px 0; }
  a { color: #2563eb; text-decoration: none; }

  img {
    max-width: 100%;
    max-height: 700px;
    width: auto;
    height: auto;
    object-fit: contain;
    margin: 12px auto;
    display: block;
    border: 1px solid #e2e8f0;
    border-radius: 6px;
    padding: 8px;
    background: #fff;
    box-shadow: 0 2px 8px rgba(0,0,0,0.08);
    page-break-inside: avoid;
  }

  .img-container {
    page-break-inside: avoid;
    text-align: center;
    margin: 8px 0;
  }

  @media print {
    body { padding: 25px 35px; }
    h2 { page-break-after: avoid; }
    table { page-break-inside: auto; }
    thead { display: table-header-group; }
    tr { page-break-inside: avoid; }
    img { max-height: 680px; }
    .img-container { page-break-inside: avoid; }
  }
`;

const html = `<!DOCTYPE html>
<html lang="vi">
<head>
<meta charset="UTF-8">
<title>${docTitle}</title>
<style>${CSS}</style>
</head>
<body>
${htmlBody}
</body>
</html>`;

const htmlFile = path.join(tempDir, 'styled.html');
fs.writeFileSync(htmlFile, html, 'utf-8');
console.log(`  ✅ HTML ready (${(fs.statSync(htmlFile).size / 1024).toFixed(0)} KB)`);

// ==================================================
// Step 5: Find or install puppeteer
// ==================================================
let puppeteer;
try {
  puppeteer = require('puppeteer');
} catch {
  let dir = __dirname;
  for (let i = 0; i < 10; i++) {
    const ppPath = path.join(dir, 'node_modules', 'puppeteer');
    if (fs.existsSync(ppPath)) {
      puppeteer = require(ppPath);
      break;
    }
    const parent = path.dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  if (!puppeteer && process.env.NODE_PATH) {
    const envPath = path.join(process.env.NODE_PATH, 'puppeteer');
    if (fs.existsSync(envPath)) puppeteer = require(envPath);
  }
  if (!puppeteer) {
    console.error('❌ puppeteer not found. Run: npm install puppeteer --no-save');
    process.exit(1);
  }
}

// ==================================================
// Step 6: Puppeteer → PDF
// ==================================================
console.log(`📄 Printing styled PDF...`);

(async () => {
  const browser = await puppeteer.launch({
    headless: 'new',
    args: ['--no-sandbox', '--disable-setuid-sandbox']
  });
  const page = await browser.newPage();
  await page.setContent(html, { waitUntil: 'networkidle0', timeout: 30000 });
  await page.pdf({
    path: outputPdf,
    format: 'A4',
    printBackground: true,
    margin: { top: '20mm', bottom: '20mm', left: '15mm', right: '15mm' },
    displayHeaderFooter: true,
    headerTemplate: `<div style="font-size:8px;color:#94a3b8;width:100%;text-align:center;padding:5px 0;">${docTitle}</div>`,
    footerTemplate: '<div style="font-size:8px;color:#94a3b8;width:100%;text-align:center;padding:5px 0;">Trang <span class="pageNumber"></span> / <span class="totalPages"></span></div>',
  });
  await browser.close();

  if (fs.existsSync(outputPdf)) {
    const sz = (fs.statSync(outputPdf).size / 1024).toFixed(0);
    console.log(`\n✅ Hoàn thành!`);
    console.log(`   📄 Output: ${outputPdf}`);
    console.log(`   📦 Size: ${sz} KB`);
    console.log(`   🎨 Mermaid diagrams: ${imageFiles.length}/${diagrams.length}`);
  } else {
    console.log(`\n❌ PDF was not created.`);
    process.exit(1);
  }
})();
