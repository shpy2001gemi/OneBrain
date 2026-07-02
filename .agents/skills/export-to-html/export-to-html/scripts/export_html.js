#!/usr/bin/env node
/**
 * Export Markdown to Styled HTML with Mermaid Rendering
 * 
 * Usage: node export_html.js <input.md> [output.html]
 * 
 * If output is not provided, uses the same name as input with .html extension.
 * 
 * Pipeline:
 *   1. Find mermaid code blocks → render to PNG via mermaid-cli
 *   2. Convert MD → styled HTML via marked + premium CSS
 *   3. Write self-contained HTML file
 * 
 * Requirements: Node.js, marked
 * Optional: @mermaid-js/mermaid-cli (only if MD contains mermaid blocks)
 */

const fs = require('fs');
const path = require('path');
const os = require('os');
const { execSync } = require('child_process');

// ─── Config ───────────────────────────────────────
const MERMAID_CLI_VERSION = '10.9.1';
const MERMAID_WIDTH = 1200;
const MERMAID_SCALE = 2;
const MERMAID_BG = 'white';
const MERMAID_TIMEOUT_MS = 120000;

function log(msg) { console.log(msg); }
function logOk(msg) { console.log(`  ✅ ${msg}`); }
function logFail(msg) { console.log(`  ❌ ${msg}`); }

// ─── Resolve module ───────────────────────────────
function resolveModule(name) {
  try { return require(name); } catch {}
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
  throw new Error(`Module '${name}' not found. Run: npm install ${name}`);
}

// ─── Premium CSS ──────────────────────────────────
const PREMIUM_CSS = `
:root {
  --blue-900: #0f172a; --blue-700: #1d4ed8; --blue-600: #2563eb;
  --blue-50: #eff6ff; --slate-700: #334155; --slate-500: #64748b;
  --slate-100: #f1f5f9; --amber-100: #fef3c7; --amber-700: #92400e;
  --amber-500: #f59e0b;
}
* { margin: 0; padding: 0; box-sizing: border-box; }
body {
  font-family: Inter, -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
  font-size: 15px; line-height: 1.7; color: #1a1a2e;
  max-width: 1100px; margin: 0 auto; padding: 40px 32px; background: #fafbfd;
}
h1 {
  font-size: 28px; font-weight: 800; color: var(--blue-900);
  border-bottom: 4px solid var(--blue-600); padding-bottom: 12px;
  margin: 40px 0 16px;
}
h1:first-child { margin-top: 0; }
h2 {
  font-size: 20px; font-weight: 700; color: #1e40af;
  padding: 10px 16px;
  background: linear-gradient(135deg, var(--blue-50), #dbeafe);
  border-left: 5px solid var(--blue-600);
  border-radius: 0 8px 8px 0; margin: 36px 0 12px;
}
h3 {
  font-size: 16px; font-weight: 600; color: var(--slate-700);
  padding: 8px 14px; background: var(--slate-100);
  border-left: 4px solid var(--slate-500);
  border-radius: 0 6px 6px 0; margin: 24px 0 8px;
}
h4 { font-size: 14px; font-weight: 600; color: #475569; margin: 18px 0 6px; }
p { margin: 6px 0; }
hr { border: none; border-top: 3px solid var(--blue-600); margin: 32px 0; opacity: 0.3; }

/* Tables */
table {
  width: 100%; border-collapse: collapse; margin: 10px 0 20px;
  border-radius: 8px; overflow: hidden;
  box-shadow: 0 1px 3px rgba(0,0,0,0.08);
}
thead th {
  background: linear-gradient(135deg, #1e3a5f, #1e40af);
  color: #fff; padding: 10px 14px; font-weight: 600;
  font-size: 13px; text-align: left; letter-spacing: 0.3px;
}
tbody td {
  padding: 8px 14px; border-bottom: 1px solid #e2e8f0;
  font-size: 14px; vertical-align: top;
}
tbody tr:nth-child(even) { background: #f0f4f8; }
tbody tr:nth-child(odd) { background: #fff; }
tbody tr:hover { background: #e0e7ff; transition: background 0.15s; }

/* Blockquotes */
blockquote {
  border-left: 4px solid var(--amber-500); background: var(--amber-100);
  padding: 12px 18px; margin: 10px 0;
  border-radius: 0 8px 8px 0; color: var(--amber-700); font-size: 14px;
}

/* Code */
pre {
  background: #1e293b; color: #e2e8f0;
  padding: 16px 20px; border-radius: 8px;
  overflow-x: auto; font-size: 13px; margin: 10px 0;
}
code { font-family: "JetBrains Mono", Consolas, "Courier New", monospace; font-size: 13px; }
p code, li code, td code {
  background: #e0e7ff; color: #3730a3;
  padding: 2px 6px; border-radius: 4px; font-size: 13px;
}

/* Misc */
strong { font-weight: 600; color: var(--blue-900); }
ul, ol { padding-left: 24px; margin: 6px 0; }
li { margin: 3px 0; }
img { max-width: 100%; height: auto; margin: 10px 0; border-radius: 8px; }

/* Print */
@media print {
  body { max-width: 100%; padding: 20px; background: #fff; }
  h2 { break-after: avoid; }
  table { break-inside: avoid; }
  tbody tr:hover { background: inherit; }
}
`;

// ─── Main ─────────────────────────────────────────
function main() {
  const inputMd = process.argv[2];
  let outputHtml = process.argv[3];

  if (!inputMd) {
    log('Usage: node export_html.js <input.md> [output.html]');
    process.exit(1);
  }
  if (!fs.existsSync(inputMd)) {
    log(`❌ Input file not found: ${inputMd}`);
    process.exit(1);
  }
  if (!outputHtml) {
    outputHtml = inputMd.replace(/\.md$/i, '.html');
  }

  log('📄 Export Markdown → Styled HTML');
  log(`   Input:  ${inputMd}`);
  log(`   Output: ${outputHtml}`);
  log('');

  let content = fs.readFileSync(inputMd, 'utf-8');
  const docTitle = path.basename(inputMd, path.extname(inputMd)).replace(/[-_]/g, ' ');

  // ─── Step 1: Find & render mermaid blocks ───────
  const mermaidRegex = /```mermaid\s*\n([\s\S]*?)```/g;
  const matches = [];
  let m;
  while ((m = mermaidRegex.exec(content)) !== null) {
    matches.push({ fullMatch: m[0], code: m[1].trim(), index: m.index, length: m[0].length });
  }

  if (matches.length > 0) {
    log(`🔍 Found ${matches.length} mermaid diagram(s)`);
    const workDir = fs.mkdtempSync(path.join(os.tmpdir(), 'mermaid_html_'));
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
          // Embed as base64 data URI for self-contained HTML
          const pngData = fs.readFileSync(pngFile).toString('base64');
          const dataUri = `data:image/png;base64,${pngData}`;
          content = content.substring(0, match.index)
            + `![Diagram ${i + 1}](${dataUri})`
            + content.substring(match.index + match.length);
        }
      } catch (err) {
        logFail(`Render error: ${(err.message || '').substring(0, 100)}`);
        // Leave as code block if render fails
      }
    }
    log(`   Rendered: ${renderedCount}/${matches.length}`);
  } else {
    log('🔍 No mermaid diagrams found');
  }

  // ─── Step 2: GitHub alerts ──────────────────────
  content = content.replace(/^> \[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\]\s*$/gm, (_, type) => {
    const emoji = { NOTE: 'ℹ️', TIP: '💡', IMPORTANT: '❗', WARNING: '⚠️', CAUTION: '🔴' }[type] || '📌';
    return `> **${emoji} ${type}:**`;
  });

  // ─── Step 3: MD → HTML ─────────────────────────
  log('📝 Converting to styled HTML...');
  const { marked } = resolveModule('marked');
  const htmlBody = marked.parse(content);

  // ─── Step 4: Assemble full HTML ────────────────
  const fullHtml = `<!DOCTYPE html>
<html lang="vi">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>${docTitle}</title>
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700;800&display=swap" rel="stylesheet">
<style>${PREMIUM_CSS}</style>
</head>
<body>
${htmlBody}
</body>
</html>`;

  fs.writeFileSync(outputHtml, fullHtml, 'utf-8');
  const sizeKb = (fs.statSync(outputHtml).size / 1024).toFixed(0);

  log('');
  log('══════════════════════════════════════');
  log(`✅ Export thành công!`);
  log(`   📄 File: ${outputHtml}`);
  log(`   📦 Size: ${sizeKb} KB`);
  log(`   🎨 Mermaid: ${matches.length > 0 ? matches.length + ' diagrams' : 'none'}`);
  log('══════════════════════════════════════');
}

main();
