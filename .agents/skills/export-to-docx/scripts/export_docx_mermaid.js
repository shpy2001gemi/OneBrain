#!/usr/bin/env node
/**
 * Export Markdown to Styled DOCX with Mermaid Rendering
 * 
 * v10 Pipeline — PDF-matching colors + _export folder:
 *   1. Find mermaid code blocks → render to PNG → save to _export/
 *   2. Clean GitHub-style alerts
 *   3. Convert MD → HTML via marked (GFM)
 *   4. Embed local images as base64 data URIs → copy to _export/
 *   5. Apply premium inline styles with table-wrapped elements
 *   6. @turbodocx/html-to-docx → DOCX (via temp file)
 *
 * Images saved to: _export/ folder next to output DOCX
 */

const fs = require('fs');
const path = require('path');
const os = require('os');
const { execSync } = require('child_process');

const MERMAID_CLI_VERSION = '10.9.1';
const MERMAID_WIDTH = 4000;
const MERMAID_SCALE = 2;
const MERMAID_BG = 'white';
const MERMAID_TIMEOUT_MS = 120000;


// Page-fit constraints for images (in pixels, at 96 DPI)
// A4 = 210mm x 297mm. With 1-inch (25.4mm) margins each side:
// Usable width  = 210 - 50.8 ≈ 159mm ≈ 601px
// Usable height = 297 - 50.8 ≈ 246mm ≈ 931px, but leave room for caption → 800px
const IMG_MAX_WIDTH = 610;
const IMG_MAX_HEIGHT = 800;

/**
 * Read image dimensions from a Buffer (supports PNG and JPEG).
 * Returns { width, height } or null if cannot determine.
 */
function getImageDimensions(buf) {
  // PNG: bytes 0-7 = signature, bytes 16-23 = IHDR width (4B) + height (4B)
  if (buf.length > 24 && buf[0] === 0x89 && buf[1] === 0x50 && buf[2] === 0x4E && buf[3] === 0x47) {
    const width = buf.readUInt32BE(16);
    const height = buf.readUInt32BE(20);
    return { width, height };
  }
  // JPEG: scan for SOF0/SOF2 markers (0xFF 0xC0 or 0xFF 0xC2)
  if (buf.length > 2 && buf[0] === 0xFF && buf[1] === 0xD8) {
    let offset = 2;
    while (offset < buf.length - 9) {
      if (buf[offset] !== 0xFF) { offset++; continue; }
      const marker = buf[offset + 1];
      if (marker === 0xC0 || marker === 0xC2) {
        const height = buf.readUInt16BE(offset + 5);
        const width = buf.readUInt16BE(offset + 7);
        return { width, height };
      }
      const segLen = buf.readUInt16BE(offset + 2);
      offset += 2 + segLen;
    }
  }
  return null;
}

/**
 * Calculate scaled dimensions to fit within maxW x maxH, maintaining aspect ratio.
 * Returns { width, height } in pixels.
 */
function fitToPage(origW, origH, maxW, maxH) {
  if (origW <= maxW && origH <= maxH) {
    return { width: origW, height: origH };
  }
  const scaleW = maxW / origW;
  const scaleH = maxH / origH;
  const scale = Math.min(scaleW, scaleH);
  return {
    width: Math.round(origW * scale),
    height: Math.round(origH * scale)
  };
}


function log(msg) { console.log(msg); }
function logOk(msg) { console.log(`  ✅ ${msg}`); }
function logFail(msg) { console.log(`  ❌ ${msg}`); }



function resolveModule(name) {
  try { return require(name); } catch { }
  let dir = __dirname;
  for (let i = 0; i < 10; i++) {
    const modPath = path.join(dir, 'node_modules', name);
    if (fs.existsSync(modPath)) return require(modPath);
    const parent = path.dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  if (process.env.NODE_PATH) {
    for (const np of process.env.NODE_PATH.split(path.delimiter)) {
      const envPath = path.join(np, name);
      if (fs.existsSync(envPath)) return require(envPath);
    }
  }
  throw new Error(`Module '${name}' not found. Run: npm install ${name} --no-save`);
}

async function main() {
  const inputMd = process.argv[2];
  const outputDocx = process.argv[3];

  if (!inputMd || !outputDocx) {
    log('Usage: node export_docx_mermaid.js <input.md> <output.docx>');
    process.exit(1);
  }
  if (!fs.existsSync(inputMd)) {
    log(`❌ Input file not found: ${inputMd}`);
    process.exit(1);
  }

  // Create _export folder next to output file
  const outputBaseName = path.basename(outputDocx, path.extname(outputDocx));
  const exportDir = path.join(path.dirname(path.resolve(outputDocx)), '_export');
  fs.mkdirSync(exportDir, { recursive: true });

  log('📄 Export Markdown → Styled DOCX (v10)');
  log(`   Input:  ${inputMd}`);
  log(`   Output: ${outputDocx}`);
  log(`   Images: ${exportDir}`);
  log('');

  let content = fs.readFileSync(inputMd, 'utf-8');
  const workDir = fs.mkdtempSync(path.join(os.tmpdir(), 'mermaid_export_'));
  const inputDir = path.dirname(path.resolve(inputMd));
  const docTitle = path.basename(inputMd, path.extname(inputMd)).replace(/[-_]/g, ' ');

  // ─── Step 1: Find & render mermaid blocks ───────
  const mermaidRegex = /```mermaid\s*\n([\s\S]*?)```/g;
  const matches = [];
  let m;
  while ((m = mermaidRegex.exec(content)) !== null) {
    matches.push({ fullMatch: m[0], code: m[1].trim(), index: m.index, length: m[0].length });
  }
  log(`🔍 Found ${matches.length} mermaid diagram(s)`);
  let renderedCount = 0;

  for (let i = matches.length - 1; i >= 0; i--) {
    const match = matches[i];
    const mmdFile = path.join(workDir, `d${i}.mmd`);
    const pngFile = path.join(workDir, `d${i}.png`);
    fs.writeFileSync(mmdFile, match.code, 'utf-8');
    log(`🎨 Rendering diagram ${i + 1}/${matches.length}...`);
    try {
      execSync(
        `npx -y @mermaid-js/mermaid-cli@${MERMAID_CLI_VERSION} mmdc -i "${mmdFile}" -o "${pngFile}" -b ${MERMAID_BG} -w ${MERMAID_WIDTH} -s ${MERMAID_SCALE}`,
        { stdio: 'pipe', timeout: MERMAID_TIMEOUT_MS }
      );
      if (fs.existsSync(pngFile) && fs.statSync(pngFile).size > 200) {
        logOk(`d${i}.png (${(fs.statSync(pngFile).size / 1024).toFixed(0)} KB)`);
        renderedCount++;
        // Save to _export folder
        const assetName = `${outputBaseName}_diagram_${String(i + 1).padStart(3, '0')}.png`;
        const assetPath = path.join(exportDir, assetName);
        fs.copyFileSync(pngFile, assetPath);
        logOk(`Saved → ${path.relative(process.cwd(), assetPath)}`);
        const b64 = fs.readFileSync(pngFile).toString('base64');
        content = content.substring(0, match.index)
          + `![Diagram ${i + 1}](data:image/png;base64,${b64})`
          + content.substring(match.index + match.length);
      }
    } catch (err) {
      logFail(`Render error: ${(err.message || '').substring(0, 100)}`);
    }
  }

  // ─── Step 2: GitHub alerts ──────────────────────
  content = content.replace(/^> \[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\]\s*$/gm, (_, type) => {
    const emoji = { NOTE: 'ℹ️', TIP: '💡', IMPORTANT: '❗', WARNING: '⚠️', CAUTION: '🔴' }[type] || '📌';
    return `> **${emoji} ${type}:**`;
  });

  // ─── Step 3: MD → HTML ─────────────────────────
  log(`\n📝 Converting to HTML...`);
  const { marked } = resolveModule('marked');
  let html = marked.parse(content);

  // ─── Step 4: Embed local images as base64 ──────
  log(`📷 Embedding local images...`);
  let imgCount = 0;
  html = html.replace(/src="([^"]+)"/g, (fullMatch, srcPath) => {
    if (srcPath.startsWith('data:') || srcPath.startsWith('http')) return fullMatch;
    const absPath = path.resolve(inputDir, decodeURIComponent(srcPath));
    if (fs.existsSync(absPath)) {
      const ext = path.extname(absPath).toLowerCase();
      const mimeMap = { '.png': 'image/png', '.jpg': 'image/jpeg', '.jpeg': 'image/jpeg', '.gif': 'image/gif', '.webp': 'image/webp' };
      const mime = mimeMap[ext] || 'image/png';
      const b64 = fs.readFileSync(absPath).toString('base64');
      imgCount++;
      logOk(`${path.basename(absPath)} (${(fs.statSync(absPath).size / 1024).toFixed(0)} KB)`);
      // Save to _export folder
      const destName = `${outputBaseName}_${path.basename(absPath)}`;
      const destPath = path.join(exportDir, destName);
      fs.copyFileSync(absPath, destPath);
      logOk(`Saved → ${path.relative(process.cwd(), destPath)}`);
      return `src="data:${mime};base64,${b64}"`;
    } else {
      logFail(`Not found: ${srcPath}`);
      return fullMatch;
    }
  });
  log(`   Embedded: ${imgCount} image(s)`);

  // ─── Step 5: Apply inline styles ───────────────
  log(`🎨 Applying inline styles...`);

  // ---- STEP 5a: Structural replacements (regex with capture groups) ----
  // MUST run BEFORE generic tag replacements to avoid double-styling

  // H2 → real heading (for Navigation) + table with blue background
  html = html.replace(/<h2>([\s\S]*?)<\/h2>/g, (_, content) => {
    const plainText = content.replace(/<[^>]+>/g, '');
    return `<h2 style="font-size:1pt;color:#dbeafe;margin:0;padding:0;line-height:1;">${plainText}</h2>`
      + `<table style="width:100%;border-collapse:collapse;margin-top:0;margin-bottom:8px;border:none;">`
      + `<tr><td style="background-color:#dbeafe;border-left:5px solid #2563eb;padding:10px 14px;border-top:none;border-right:none;border-bottom:none;">`
      + `<strong style="font-size:16pt;color:#1e40af;">${content}</strong>`
      + `</td></tr></table>`;
  });

  // H3 → real heading (for Navigation) + table with gray background
  html = html.replace(/<h3>([\s\S]*?)<\/h3>/g, (_, content) => {
    const plainText = content.replace(/<[^>]+>/g, '');
    return `<h3 style="font-size:1pt;color:#f1f5f9;margin:0;padding:0;line-height:1;">${plainText}</h3>`
      + `<table style="width:100%;border-collapse:collapse;margin-top:0;margin-bottom:6px;border:none;">`
      + `<tr><td style="background-color:#f1f5f9;border-left:4px solid #64748b;padding:8px 12px;border-top:none;border-right:none;border-bottom:none;">`
      + `<strong style="font-size:13pt;color:#334155;">${content}</strong>`
      + `</td></tr></table>`;
  });

  // Blockquotes → table with amber background + left border
  html = html.replace(/<blockquote>([\s\S]*?)<\/blockquote>/g, (_, content) =>
    `<table style="width:100%;border-collapse:collapse;margin:8px 0;border:none;">`
    + `<tr><td style="background-color:#fef3c7;border-left:4px solid #f59e0b;padding:10px 14px;color:#92400e;border-top:none;border-right:none;border-bottom:none;">`
    + `${content}</td></tr></table>`);

  // Code blocks → table with dark header + p-tag lines
  html = html.replace(/<pre><code[^>]*>([\s\S]*?)<\/code><\/pre>/g, (_, codeContent) => {
    const lines = codeContent.replace(/\n$/, '').split('\n');
    const lineHtml = lines.map(line => {
      let formatted = line
        .replace(/\t/g, '    ')
        .replace(/^ +/, m => '&nbsp;'.repeat(m.length))
        .replace(/  /g, ' &nbsp;');
      if (formatted === '') formatted = '&nbsp;';
      return `<p style="margin:0;padding:0;line-height:1.3;font-family:Consolas;font-size:9pt;color:#1e293b;">${formatted}</p>`;
    }).join('');
    return `<table style="width:100%;border-collapse:collapse;margin:8px 0 12px 0;border:2px solid #334155;">`
      + `<tr><td style="background-color:#1e293b;color:#94a3b8;font-family:Consolas;font-size:8pt;font-weight:bold;padding:4px 12px;border-bottom:1px solid #475569;">CODE</td></tr>`
      + `<tr><td style="background-color:#f8fafc;padding:10px 14px;">${lineHtml}</td></tr>`
      + `</table>`;
  });

  // ---- STEP 5b: Simple tag replacements ----

  // H1, H4 (not table-wrapped)
  html = html
    .replace(/<h1>/g, '<h1 style="font-size:22pt;font-weight:bold;color:#0f172a;border-bottom:3px solid #2563eb;padding-bottom:8px;margin-top:24px;margin-bottom:8px;">')
    .replace(/<h4>/g, '<h4 style="font-size:11pt;font-weight:bold;color:#475569;margin-top:12px;margin-bottom:4px;">');

  // Data tables — navy header
  html = html
    .replace(/<table>/g, '<table style="width:100%;border-collapse:collapse;margin-top:8px;margin-bottom:14px;border:1px solid #cbd5e0;">')
    .replace(/<th>/g, '<th style="background-color:#1e3a5f;color:#ffffff;font-size:10pt;font-weight:bold;padding:8px 12px;text-align:left;border:1px solid #1e3a5f;">')
    .replace(/<td>/g, '<td style="font-size:10pt;padding:6px 12px;border:1px solid #e2e8f0;vertical-align:top;">');

  // Zebra rows
  let rowCount = 0;
  let inThead = false;
  html = html.replace(/<thead>|<\/thead>|<tr>/g, (tag) => {
    if (tag === '<thead>') { inThead = true; return tag; }
    if (tag === '</thead>') { inThead = false; rowCount = 0; return tag; }
    if (inThead) return '<tr style="background-color:#1e3a5f;">';
    rowCount++;
    return `<tr style="background-color:${rowCount % 2 === 0 ? '#f0f4f8' : '#ffffff'};">`;
  });

  // Inline code
  html = html.replace(/<code>/g, '<code style="font-family:Consolas;font-size:9pt;background-color:#e0e7ff;color:#3730a3;padding:1px 4px;">');

  // Text elements
  html = html
    .replace(/<hr\s*\/?>/g, '<hr style="border:none;border-top:3px solid #2563eb;margin:20px 0;">')
    .replace(/<p>/g, '<p style="font-size:11pt;line-height:1.6;margin:4px 0;">')
    .replace(/<li>/g, '<li style="font-size:11pt;line-height:1.6;margin:2px 0;">')
    .replace(/<strong>/g, '<strong style="font-weight:bold;color:#0f172a;">')
    .replace(/<a /g, '<a style="color:#2563eb;" ');

  // ---- STEP 5c: Scale images to fit within one page ----
  log(`📐 Scaling images to fit page...`);
  html = html.replace(/<img\s+([^>]*)>/g, (fullMatch, attrs) => {
    // Extract src to read dimensions
    const srcMatch = attrs.match(/src="([^"]+)"/);
    if (!srcMatch) return `<img style="max-width:100%;height:auto;margin:8px 0;" ${attrs}>`;
    const src = srcMatch[1];
    let dims = null;
    if (src.startsWith('data:image/')) {
      // base64 embedded image — decode to get dimensions
      const b64Match = src.match(/^data:image\/[^;]+;base64,(.+)$/);
      if (b64Match) {
        try {
          const imgBuf = Buffer.from(b64Match[1], 'base64');
          dims = getImageDimensions(imgBuf);
        } catch { }
      }
    }
    if (dims && (dims.width > IMG_MAX_WIDTH || dims.height > IMG_MAX_HEIGHT)) {
      const fitted = fitToPage(dims.width, dims.height, IMG_MAX_WIDTH, IMG_MAX_HEIGHT);
      logOk(`Scaled ${dims.width}x${dims.height} → ${fitted.width}x${fitted.height}`);
      // Remove any existing width/height attributes
      let cleanAttrs = attrs.replace(/\bwidth="[^"]*"/g, '').replace(/\bheight="[^"]*"/g, '');
      return `<img width="${fitted.width}" height="${fitted.height}" style="margin:8px 0;" ${cleanAttrs.trim()}>`;
    }
    // No scaling needed or can't determine dimensions — use max-width fallback
    return `<img style="max-width:100%;height:auto;margin:8px 0;" ${attrs}>`;
  });


  // Build HTML
  const fullHtml = `<!DOCTYPE html>
<html><head><meta charset="UTF-8"><title>${docTitle}</title></head>
<body style="font-size:11pt;line-height:1.6;color:#1a1a2e;">
${html}
</body></html>`;

  const htmlFile = path.join(workDir, 'styled.html');
  fs.writeFileSync(htmlFile, fullHtml, 'utf-8');
  logOk(`HTML ready (${(fs.statSync(htmlFile).size / 1024).toFixed(0)} KB)`);

  // ─── Step 6: turbodocx → DOCX ─────────────────
  log(`\n📝 Converting HTML → DOCX (turbodocx v8)...`);
  let HTMLtoDOCX;
  try {
    HTMLtoDOCX = resolveModule('@turbodocx/html-to-docx');
  } catch {
    HTMLtoDOCX = resolveModule('html-to-docx');
  }

  try {
    const docxBuffer = await HTMLtoDOCX(fullHtml, null, {
      title: docTitle,
      margins: { top: 720, right: 720, bottom: 720, left: 720 },
    });

    // Write to temp first (avoids path-with-spaces issues), then copy
    const tempDocx = path.join(workDir, 'output.docx');
    fs.writeFileSync(tempDocx, docxBuffer);
    fs.copyFileSync(tempDocx, outputDocx);

    if (fs.existsSync(outputDocx)) {
      const sizeKb = (fs.statSync(outputDocx).size / 1024).toFixed(0);
      log('');
      log('══════════════════════════════════════');
      log(`✅ Export thành công!`);
      log(`   📄 File: ${outputDocx}`);
      log(`   📦 Size: ${sizeKb} KB`);
      log(`   🎨 Diagrams: ${renderedCount}/${matches.length}`);
      log(`   📷 Images: ${imgCount}`);
      log(`   📂 Images: ${exportDir}`);
      log('══════════════════════════════════════');
    } else {
      logFail('DOCX not created');
      process.exit(1);
    }
  } catch (err) {
    logFail(`Error: ${err.message}`);
    console.log(err.stack);
    process.exit(1);
  }

  log(`\n🗑️  Temp: ${workDir}`);
}

main().catch(err => {
  logFail(`Fatal: ${err.message}`);
  console.log(err.stack);
  process.exit(1);
});
