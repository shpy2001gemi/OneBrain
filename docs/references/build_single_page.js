const fs = require('fs');
const path = require('path');
const os = require('os');
const { execSync } = require('child_process');

const MERMAID_CLI_VERSION = '10.9.1';
const MERMAID_TIMEOUT_MS = 120000;

function log(msg) { console.log(msg); }
function logOk(msg) { console.log(`  ✅ ${msg}`); }
function logFail(msg) { console.log(`  ❌ ${msg}`); }

// ─── Resolve marked module ──────────────────────────
let markedModule;
try {
  markedModule = require('marked');
} catch (e) {
  const workspaceMarked = path.join(__dirname, '..', '..', 'node_modules', 'marked');
  if (fs.existsSync(workspaceMarked)) {
    markedModule = require(workspaceMarked);
  } else {
    throw new Error('marked module not found. Run npm install marked.');
  }
}
const { marked } = markedModule;

// ─── Path Config ────────────────────────────────────
const baseDir = path.join(__dirname, '..', '..');
const readmePath = path.join(baseDir, 'docs', 'README.md');
const paperViDir = path.join(baseDir, 'docs', 'references', 'paper-vi');
const outputPath = path.join(baseDir, 'docs', 'references', 'index.html');

// ─── Helper to render Mermaid diagram to base64 ─────
function renderMermaidToBase64(mermaidCode, id, workDir) {
  const mmdFile = path.join(workDir, `${id}.mmd`);
  const pngFile = path.join(workDir, `${id}.png`);
  fs.writeFileSync(mmdFile, mermaidCode, 'utf-8');
  try {
    execSync(
      `npx -y @mermaid-js/mermaid-cli@${MERMAID_CLI_VERSION} mmdc -i "${mmdFile}" -o "${pngFile}" -b white -w 1200 -s 2`,
      { stdio: 'pipe', timeout: MERMAID_TIMEOUT_MS }
    );
    if (fs.existsSync(pngFile) && fs.statSync(pngFile).size > 200) {
      const pngData = fs.readFileSync(pngFile).toString('base64');
      return `data:image/png;base64,${pngData}`;
    }
  } catch (err) {
    logFail(`Mermaid render failed for ${id}: ${err.message}`);
  }
  return null;
}

// ─── Process Markdown text ─────────────────────────
function processMarkdown(content, fileId, workDir) {
  // Replace Mermaid blocks
  const mermaidRegex = /```mermaid\s*\n([\s\S]*?)```/g;
  let matches = [];
  let m;
  while ((m = mermaidRegex.exec(content)) !== null) {
    matches.push({ fullMatch: m[0], code: m[1].trim(), index: m.index, length: m[0].length });
  }

  if (matches.length > 0) {
    log(`  🎨 Rendering ${matches.length} Mermaid diagram(s) for ${fileId}...`);
    for (let i = matches.length - 1; i >= 0; i--) {
      const match = matches[i];
      const base64Uri = renderMermaidToBase64(match.code, `${fileId}_m${i}`, workDir);
      if (base64Uri) {
        content = content.substring(0, match.index)
          + `![Diagram ${i + 1}](${base64Uri})`
          + content.substring(match.index + match.length);
      }
    }
  }

  // Convert GitHub alerts
  content = content.replace(/^> \[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\]\s*$/gm, (_, type) => {
    const emoji = { NOTE: 'ℹ️', TIP: '💡', IMPORTANT: '❗', WARNING: '⚠️', CAUTION: '🔴' }[type] || '📌';
    return `> **${emoji} ${type}:**`;
  });

  return content;
}

// ─── Light Mode Stylesheet for Landing Page + Reader ─
const LIGHT_CSS = `
:root {
  --bg-body: #f8fafc;
  --bg-card: #ffffff;
  --bg-content: #ffffff;
  --text-main: #0f172a;
  --text-muted: #475569;
  --border-color: #e2e8f0;
  
  --primary: #6366f1;
  --primary-hover: #4f46e5;
  --primary-bg: #eef2ff;
  
  --accent-green: #10b981;
  --accent-green-bg: #d1fae5;
  --accent-orange: #f97316;
  --accent-yellow: #eab308;
  --accent-red: #ef4444;
  
  --font-heading: 'Outfit', 'Inter', sans-serif;
  --font-body: 'Inter', sans-serif;
}

* {
  box-sizing: border-box;
  margin: 0;
  padding: 0;
}

html, body {
  max-width: 100%;
  overflow-x: hidden;
}

body {
  background-color: var(--bg-body);
  color: var(--text-main);
  font-family: var(--font-body);
  line-height: 1.6;
  font-size: 15px;
}

/* Landing Page container */
.container {
  max-width: 1100px;
  margin: 0 auto;
  padding: 0 1.5rem 4rem 1.5rem;
}

/* Hero Section */
.hero {
  text-align: center;
  padding: 6rem 0 4rem 0;
  background: radial-gradient(circle at top, rgba(99, 102, 241, 0.08) 0%, transparent 65%);
}

.logo-badge {
  display: inline-flex;
  align-items: center;
  background: var(--primary-bg);
  color: var(--primary);
  padding: 0.5rem 1.25rem;
  border-radius: 9999px;
  font-weight: 700;
  font-size: 0.85rem;
  margin-bottom: 1.5rem;
  border: 1px solid rgba(99, 102, 241, 0.15);
  font-family: var(--font-heading);
}

.hero h1 {
  font-family: var(--font-heading);
  font-size: 3.5rem;
  font-weight: 800;
  letter-spacing: -0.03em;
  color: #0f172a;
  line-height: 1.15;
  margin-bottom: 1.25rem;
}

.hero p {
  font-size: 1.2rem;
  color: var(--text-muted);
  max-width: 750px;
  margin: 0 auto 3rem auto;
  font-weight: 400;
}

/* Project Stats */
.project-meta {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
  gap: 1.25rem;
  margin-bottom: 4rem;
}

.meta-card {
  background: #ffffff;
  border: 1px solid var(--border-color);
  border-radius: 20px;
  padding: 1.5rem;
  box-shadow: 0 4px 6px -1px rgba(0, 0, 0, 0.02), 0 2px 4px -1px rgba(0, 0, 0, 0.01);
  transition: transform 0.2s;
}

.meta-card:hover {
  transform: translateY(-2px);
}

.meta-card .label {
  font-size: 0.8rem;
  color: var(--text-muted);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  margin-bottom: 0.35rem;
}

.meta-card .value {
  font-family: var(--font-heading);
  font-size: 1.5rem;
  font-weight: 700;
  color: var(--primary-hover);
}

/* Overview Section */
.overview-section {
  background: #ffffff;
  border: 1px solid var(--border-color);
  border-radius: 24px;
  padding: 3.5rem;
  margin-bottom: 4rem;
  box-shadow: 0 4px 20px -2px rgba(0, 0, 0, 0.02);
}

/* Sections title */
.section-title {
  font-family: var(--font-heading);
  font-size: 2rem;
  font-weight: 700;
  color: #0f172a;
  margin-bottom: 2rem;
  display: flex;
  align-items: center;
  gap: 0.75rem;
}

.section-title::before {
  content: '';
  display: inline-block;
  width: 4px;
  height: 28px;
  background: var(--primary);
  border-radius: 2px;
}

/* Pillars grid */
.pillars-grid {
  display: grid;
  grid-template-columns: 1fr;
  gap: 2rem;
  margin-bottom: 5rem;
}

.pillar-card {
  background: #ffffff;
  border: 1px solid var(--border-color);
  border-radius: 24px;
  padding: 2.25rem;
  box-shadow: 0 4px 30px rgba(0, 0, 0, 0.015);
  transition: all 0.3s cubic-bezier(0.4, 0, 0.2, 1);
  position: relative;
  overflow: hidden;
}

.pillar-card:hover {
  transform: translateY(-3px);
  box-shadow: 0 12px 40px rgba(99, 102, 241, 0.06);
  border-color: rgba(99, 102, 241, 0.35);
}

.pillar-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  flex-wrap: wrap;
  gap: 1rem;
  margin-bottom: 1.25rem;
}

.pillar-title-group {
  display: flex;
  align-items: center;
  gap: 1rem;
}

.pillar-number {
  font-family: var(--font-heading);
  font-size: 1.1rem;
  font-weight: 800;
  color: var(--primary);
  background: var(--primary-bg);
  padding: 0.35rem 0.85rem;
  border-radius: 10px;
  border: 1px solid rgba(99, 102, 241, 0.15);
}

.pillar-title {
  font-family: var(--font-heading);
  font-size: 1.6rem;
  font-weight: 700;
  color: #0f172a;
}

.badge-status {
  display: inline-flex;
  align-items: center;
  padding: 0.35rem 0.85rem;
  border-radius: 20px;
  font-size: 0.8rem;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.025em;
}

.badge-completed {
  background: #d1fae5;
  color: #065f46;
  border: 1px solid rgba(16, 185, 129, 0.2);
}

.badge-nearly {
  background: #d1fae5;
  color: #047857;
  border: 1px solid rgba(16, 185, 129, 0.15);
}

.badge-research {
  background: #ffedd5;
  color: #9a3412;
  border: 1px solid rgba(249, 115, 22, 0.2);
}

.badge-developing {
  background: #fef9c3;
  color: #854d0e;
  border: 1px solid rgba(234, 179, 8, 0.2);
}

.badge-vision {
  background: #fee2e2;
  color: #991b1b;
  border: 1px solid rgba(239, 68, 68, 0.2);
}

.pillar-description {
  color: var(--text-muted);
  font-size: 0.975rem;
  margin-bottom: 1.75rem;
}

/* Paper Path Links */
.paper-section {
  margin-top: 1.5rem;
  background: #f8fafc;
  border: 1px solid var(--border-color);
  border-radius: 18px;
  padding: 1.5rem;
}

.paper-section-title {
  font-family: var(--font-heading);
  font-size: 0.9rem;
  font-weight: 700;
  color: #1e293b;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  margin-bottom: 1rem;
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.paper-path-flow {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 0.5rem;
  background: #ffffff;
  padding: 0.75rem 1.25rem;
  border-radius: 12px;
  border: 1px solid var(--border-color);
}

.path-root {
  font-weight: 700;
  color: var(--primary);
  font-size: 0.85rem;
}

.path-separator {
  color: #cbd5e1;
  font-size: 0.8rem;
  user-select: none;
}

.path-pillar-name {
  font-weight: 600;
  color: #1e293b;
  font-size: 0.85rem;
}

.path-link {
  color: var(--text-muted);
  text-decoration: none;
  font-size: 0.85rem;
  padding: 0.3rem 0.6rem;
  border-radius: 6px;
  background: #f1f5f9;
  border: 1px solid #e2e8f0;
  transition: all 0.2s ease;
  white-space: nowrap;
  cursor: pointer;
  font-weight: 500;
}

.path-link:hover {
  color: #fff;
  background: var(--primary);
  border-color: transparent;
  box-shadow: 0 4px 12px rgba(99, 102, 241, 0.25);
  transform: translateY(-1px);
}

/* Menu toggle button (hidden by default on desktop) */
.btn-menu-toggle {
  display: none;
}

/* Reader Fullscreen Overlay */
.reader-overlay {
  position: fixed;
  top: 0;
  left: 0;
  width: 100vw;
  height: 100vh;
  background-color: var(--bg-body);
  z-index: 2000;
  display: none;
  flex-direction: row;
  overflow: hidden;
}

.reader-overlay.active {
  display: flex;
}

.reader-sidebar {
  width: 320px;
  background-color: #ffffff;
  border-right: 1px solid var(--border-color);
  display: flex;
  flex-direction: column;
  height: 100%;
  flex-shrink: 0;
}

.reader-sidebar-header {
  padding: 1.5rem;
  border-bottom: 1px solid var(--border-color);
}

.btn-close {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 0.5rem;
  width: 100%;
  padding: 0.65rem 1rem;
  background-color: #f1f5f9;
  border: 1px solid #e2e8f0;
  border-radius: 8px;
  font-weight: 600;
  font-size: 0.85rem;
  color: var(--text-main);
  cursor: pointer;
  transition: all 0.2s;
  margin-bottom: 1.25rem;
  font-family: var(--font-heading);
}

.btn-close:hover {
  background-color: #e2e8f0;
}

.reader-pillar-name {
  font-family: var(--font-heading);
  font-size: 1.15rem;
  font-weight: 700;
  color: var(--primary);
}

.reader-menu {
  flex: 1;
  overflow-y: auto;
  padding: 1rem 0;
}

.reader-menu-item {
  padding: 0.6rem 1.5rem;
  font-size: 0.9rem;
  color: var(--text-main);
  cursor: pointer;
  border-left: 3px solid transparent;
  transition: all 0.15s;
  font-weight: 500;
}

.reader-menu-item:hover {
  background: #f1f5f9;
  color: var(--primary);
}

.reader-menu-item.active {
  background: var(--primary-bg);
  color: var(--primary);
  border-left-color: var(--primary);
  font-weight: 600;
}

.reader-main {
  flex: 1;
  display: flex;
  flex-direction: column;
  height: 100%;
  background: #ffffff;
}

.reader-top-bar {
  height: 60px;
  border-bottom: 1px solid var(--border-color);
  display: flex;
  align-items: center;
  padding: 0 3rem;
  font-family: var(--font-heading);
  font-size: 0.95rem;
  font-weight: 600;
  background-color: #ffffff;
}

.reader-breadcrumb {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.reader-bc-current {
  color: var(--primary);
}

.reader-content-scroll {
  flex: 1;
  overflow-y: auto;
  padding: 3rem 5rem;
}

.reader-content-body {
  max-width: 800px;
  margin: 0 auto;
}

.chapter-pane {
  display: none;
}

.chapter-pane.active {
  display: block;
  animation: fadeIn 0.2s ease-out;
}

@keyframes fadeIn {
  from { opacity: 0; transform: translateY(3px); }
  to { opacity: 1; transform: translateY(0); }
}

/* Markdown styling inside readers */
.markdown-body h1 {
  font-family: var(--font-heading);
  font-size: 2.2rem;
  font-weight: 800;
  margin-bottom: 1.5rem;
  color: #0f172a;
  border-bottom: 3px solid var(--primary);
  padding-bottom: 0.5rem;
}

.markdown-body h2 {
  font-family: var(--font-heading);
  font-size: 1.5rem;
  font-weight: 700;
  margin: 2.2rem 0 1rem;
  color: #1e3a8a;
  border-left: 4px solid var(--primary);
  padding-left: 0.75rem;
}

.markdown-body h3 {
  font-family: var(--font-heading);
  font-size: 1.2rem;
  font-weight: 600;
  margin: 1.5rem 0 0.75rem;
  color: #334155;
  border-left: 3px solid var(--text-muted);
  padding-left: 0.6rem;
}

.markdown-body p {
  margin-bottom: 1rem;
}

.markdown-body ul, .markdown-body ol {
  padding-left: 1.75rem;
  margin-bottom: 1rem;
}

.markdown-body li {
  margin-bottom: 0.25rem;
}

.markdown-body hr {
  border: none;
  border-top: 1px solid var(--border-color);
  margin: 2.5rem 0;
}

.markdown-body table {
  display: block;
  width: 100%;
  overflow-x: auto;
  -webkit-overflow-scrolling: touch;
  border-collapse: collapse;
  margin: 1.5rem 0;
  border-radius: 8px;
  box-shadow: 0 1px 3px rgba(0,0,0,0.05);
}

.markdown-body thead th {
  background: var(--primary);
  color: #fff;
  padding: 10px 14px;
  font-weight: 600;
  font-size: 13px;
  text-align: left;
}

.markdown-body tbody td {
  padding: 8px 14px;
  border-bottom: 1px solid var(--border-color);
  font-size: 14px;
}

.markdown-body tbody tr:nth-child(even) {
  background: #f8fafc;
}

.markdown-body tbody tr:hover {
  background: #f1f5f9;
}

.markdown-body blockquote {
  border-left: 4px solid var(--accent-yellow);
  background: #fefce8;
  padding: 1rem 1.25rem;
  margin: 1.5rem 0;
  border-radius: 0 8px 8px 0;
  color: #854d0e;
}

.markdown-body pre {
  background: #1e293b;
  color: #e2e8f0;
  padding: 1.25rem;
  border-radius: 8px;
  overflow-x: auto;
  margin: 1.5rem 0;
}

.markdown-body code {
  font-family: "JetBrains Mono", Consolas, monospace;
  font-size: 0.85rem;
}

.markdown-body p code, .markdown-body li code, .markdown-body td code {
  background: #eef2ff;
  color: #4338ca;
  padding: 2px 6px;
  border-radius: 4px;
}

.markdown-body img {
  max-width: 100%;
  height: auto;
  margin: 1.5rem auto;
  display: block;
  border-radius: 12px;
  box-shadow: 0 10px 25px -5px rgba(0, 0, 0, 0.05);
  border: 1px solid var(--border-color);
}

footer {
  text-align: center;
  padding: 4rem 0 2rem 0;
  border-top: 1px solid var(--border-color);
  color: var(--text-muted);
  font-size: 0.9rem;
}

footer p {
  margin-bottom: 0.5rem;
}

@media (max-width: 768px) {
  .hero { padding: 3rem 0 2rem 0; }
  .hero h1 { font-size: 2.2rem; }
  .project-meta { grid-template-columns: 1fr; gap: 1rem; margin-bottom: 2.5rem; }
  .overview-section { padding: 2rem 1.25rem; margin-bottom: 2.5rem; }
  .pillars-grid { gap: 1.5rem; margin-bottom: 3rem; }
  .pillar-card { padding: 1.5rem; }
  .paper-path-flow { flex-direction: column; align-items: flex-start; }
  .path-separator { display: none; }
  .reader-overlay { flex-direction: column; }
  .reader-sidebar { width: 100%; height: auto; border-right: none; border-bottom: 1px solid var(--border-color); }
  .reader-content-scroll { padding: 1.5rem; }

  /* Mobile Menu Styles */
  .reader-sidebar-title-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-top: 0.75rem;
  }
  .btn-menu-toggle {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    padding: 0.4rem 0.75rem;
    background-color: var(--primary-bg);
    border: 1px solid rgba(99, 102, 241, 0.2);
    border-radius: 6px;
    font-weight: 600;
    font-size: 0.8rem;
    color: var(--primary);
    cursor: pointer;
    transition: all 0.2s;
  }
  .btn-menu-toggle:hover {
    background-color: var(--primary);
    color: #ffffff;
  }
  .reader-menu {
    display: none;
    flex: none;
    max-height: 240px;
    overflow-y: auto;
    border-bottom: 1px solid var(--border-color);
    padding: 0.5rem 0;
  }
  .reader-menu.expanded {
    display: block;
  }
}
`;

// ─── Main Program ───────────────────────────────────
function main() {
  log('🚀 Starting OneBrain Landing Page + Reader compiler...');
  
  const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'onebrain_compile_lp_'));
  log(`  📁 Temp directory: ${tempDir}`);
  
  // 1. Read & process docs/README.md
  log('  📖 Reading docs/README.md...');
  let readmeContent = fs.readFileSync(readmePath, 'utf-8');
  
  // Replace 5 pillars architecture block with 10 pillars architecture details
  const archStart = readmeContent.indexOf('## Architecture — 5 Pillars');
  const archEnd = readmeContent.indexOf('## Quick Start');
  if (archStart !== -1 && archEnd !== -1) {
    const archBlock = readmeContent.substring(archStart, archEnd);
    const newArchBlock = `## Kiến trúc hệ thống — 10 Trụ cột công nghệ

\`\`\`mermaid
graph TD
    P1["🧬 P1: KU Core<br/>(Hạt nhân Trung tâm)"]
    P2["P2: Network Protocol"]
    P3["P3: KQL Query"]
    P4["P4: Consensus PoMV"]
    P5["P5: OBT Token"]
    P6["P6: AI Layer"]
    P7["P7: Knowledge Graph"]
    P8["P8: Storage Layer"]
    P9["P9: BCI Protocol"]
    P10["P10: User Interface"]

    %% Quan hệ cốt lõi
    P1 --> P4
    P4 -->|Rewards| P5
    P5 -->|Incentives| P1
    P4 --> P3
    P3 --> P2
    P2 -->|Gossip & Sync| P1

    %% Các hệ thống bổ trợ kết nối tới trung tâm
    P6 -->|Auto Encode| P1
    P7 -->|Index & Links| P1
    P7 -->|Graph Query| P3
    P8 -->|ACID Persistence| P1
    P9 -->|Direct Sync| P1
    P10 -->|Query Input| P3
    P10 -->|Visualization| P7

    %% Phong cách hiển thị
    style P1 fill:#10b981,color:#fff,stroke:#047857,stroke-width:4px
    style P2 fill:#34d399,color:#064e3b,stroke:#059669
    style P3 fill:#34d399,color:#064e3b,stroke:#059669
    style P4 fill:#34d399,color:#064e3b,stroke:#059669
    style P5 fill:#34d399,color:#064e3b,stroke:#059669
    style P6 fill:#ffedd5,color:#9a3412,stroke:#f97316
    style P7 fill:#fef9c3,color:#854d0e,stroke:#eab308
    style P8 fill:#d1fae5,color:#065f46,stroke:#10b981
    style P9 fill:#f1f5f9,color:#475569,stroke:#cbd5e1
    style P10 fill:#f1f5f9,color:#475569,stroke:#cbd5e1
\`\`\`

| Trụ cột | Crate | Mô tả | Trạng thái |
|--------|-------|--------|------------|
| **P1: KU Core** | \`ku-core\` | 3-layer architecture: CoreDna (binary instructions) → Epigenetics (trust/bonds) → Expression (natural language) | 🟢 Đã hoàn thành (98%) |
| **P2: Network Protocol** | \`ku-net\` | OneBrain Protocol — identity, SWIM membership, Kademlia DHT, stigmergy routing | 🟢 Đã hoàn thành (95%) |
| **P3: KQL** | \`ku-kql\` | Ngôn ngữ truy vấn KQL — FIND, CREATE, UPDATE, DEPRECATE, WATCH, EXPLAIN | 🟢 Đã hoàn thành (90%) |
| **P4: Consensus PoMV** | \`ku-core\` | Proof-of-Metabolic-Value — Cơ chế đồng thuận dựa trên 6 chỉ số quan sát phi tập trung | 🟢 Đã hoàn thành (85%) |
| **P5: OBT Token** | \`ku-core\` | Kinh tế học Token OBT — Phần thưởng 4 dòng, sổ cái Account-Chain, ngăn chặn spam | 🟢 Đã hoàn thành (75%) |
| **P6: AI Layer** | \`ku-core\` | Trình biên dịch tự động (Text → CoreDna), phân loại tri thức, phát hiện trùng lặp | 🟡 Đang nghiên cứu (30%) |
| **P7: Knowledge Graph** | \`ku-net\` | Đồ thị liên kết tri thức 33 loại Bonds, Synaptic Maps, Gap/Bridge Detection | 🟡 Đang phát triển (45%) |
| **P8: Storage Layer** | \`ku-kql\` | Lưu trữ redb cục bộ bền vững, ACID, định danh nội dung (Content-addressed) | 🟢 Đã có nền tảng (65%) |
| **P9: BCI Protocol** | — | Giao thức não bộ - máy tính, ghi nhận và đồng bộ tri thức trực tiếp từ vỏ não | 🔴 Tầm nhìn xa (15%) |
| **P10: User Interface** | \`ku-demo\` | Giao diện đồ họa xem bản đồ tri thức, dashboard token, hiện tại mới có CLI demo | 🔴 Tầm nhìn xa (10%) |

`;
    readmeContent = readmeContent.replace(archBlock, newArchBlock);
  }
  
  // Translate main contents to Vietnamese
  readmeContent = readmeContent
    .replace('# OneBrain — Decentralized Knowledge Management', '# Hệ sinh thái Tri thức Phi tập trung OneBrain')
    .replace('> **Quản lý tri thức phi tập trung với kiến trúc lấy cảm hứng sinh học.**', '> **Hệ sinh thái quản lý và chia sẻ tri thức phi tập trung với cấu trúc lấy cảm hứng sinh học.**')
    .replace('OneBrain encodes human knowledge into compact, language-agnostic **Knowledge Units (KU)** that live, metabolize, and evolve across a peer-to-peer network — no central servers, no cloud dependency.', 'OneBrain mã hóa tri thức nhân loại thành các đơn vị tri thức nhỏ gọn và không phụ thuộc ngôn ngữ gọi là **Knowledge Units (KU)**. Các KU này tồn tại, chuyển hóa sinh học (metabolize) và tự tiến hóa trong mạng lưới peer-to-peer (P2P) — không máy chủ trung tâm, không phụ thuộc vào đám mây.')
    .replace('## Architecture — 5 Pillars', '## Kiến trúc — 5 Trụ cột chính')
    .replace('### P5: OBT Token (OneBrain Token)', '### P5: OBT Token (OneBrain Token) - Kinh tế học Token')
    .replace('Incentive mechanism that rewards knowledge contribution, encoding, verification, and storage.', 'Cơ chế khuyến khích chi trả phần thưởng cho đóng góp tri thức, mã hóa, xác thực và lưu trữ.')
    .replace('- 4-stream rewards: PoMV (R1), Encoding (R2), Verification (R3), Storage (R4)', '- 4 dòng phần thưởng: PoMV (R1), Mã hóa (R2), Xác thực (R3), Lưu trữ (R4)')
    .replace('- Account-Chain ledger (Nano-style, no global blockchain)', '- Sổ cái Account-Chain (tương tự Nano, không có blockchain toàn cục)')
    .replace('- 7-tier NodeTier hierarchy with EigenTrust-gated promotions', '- Phân cấp NodeTier 7 lớp với bầu chọn dựa trên EigenTrust')
    .replace('- 5-tier graduated penalty system (fraud → tombstone)', '- Hệ thống hình phạt phân bậc 5 tầng (từ gian lận đến đánh dấu khai tử/tombstone)')
    .replace('## Quick Start', '## Bắt đầu Nhanh')
    .replace('## Specification Documents', '## Tài liệu Đặc tả Kỹ thuật')
    .replace('### Cross-cutting Documents', '### Các Tài liệu Tham khảo khác')
    .replace('## Code ↔ Documentation Cross-Reference', '## Đối chiếu giữa Source Code và Tài liệu')
    .replace('### Workspace Crates', '### Các Crate trong Workspace')
    .replace('### P1: KU Core Modules', '### Mô-đun Core của KU (Pillar 1)')
    .replace('### P2: PoK/PoMV Modules', '### Mô-đun PoMV Consensus (Pillar 2/4)')
    .replace('### P2.5: AI Integration Modules', '### Mô-đun Tích hợp AI')
    .replace('### P3: KQL Modules', '### Mô-đun KQL Query (Pillar 3)')
    .replace('### P4: OBP Network Modules', '### Mô-đun Mạng lưới OBP (Pillar 2)')
    .replace('### P5: OBT Token Modules', '### Mô-đun Token OBT (Pillar 5)')
    .replace('## Key Dependencies', '## Các thư viện phụ thuộc chính');
  
  readmeContent = processMarkdown(readmeContent, 'readme', tempDir);
  
  const splitMarker = '## Bắt đầu Nhanh';
  const readmeParts = readmeContent.split(splitMarker);
  const readmeHtmlPart1 = marked.parse(readmeParts[0]);
  const readmeHtmlPart2 = marked.parse(splitMarker + '\n' + (readmeParts[1] || ''));

  // 2. Pillars list in order
  const pillars = [
    { id: 'ku', name: 'Knowledge Unit (KU)', code: 'P1', badgeClass: 'badge-completed', progress: 'Hoàn thiện 98%', hasPaper: true, desc: 'Đơn vị cơ bản nhất đại diện cho tri thức của OneBrain, mã hóa bằng Core DNA v6 nhị phân tự mô tả. Hỗ trợ 10 loại Gene, 33 loại Bond, 11 cấp độ nhận thức và Delta-state CRDT cho tính đồng bộ cuối cùng.', folder: 'ku' },
    { id: 'network', name: 'Network Protocol (OBP)', code: 'P2', badgeClass: 'badge-completed', progress: 'Hoàn thiện 95%', hasPaper: true, desc: 'Giao thức mạng 9 tầng phi tập trung tích hợp QUIC transport thực tế, cơ chế Membership SWIM, Kademlia DHT, và Stigmergy (pheromone routing lấy cảm hứng từ kiến) để tìm kiếm và định tuyến dữ liệu thông minh qua mạng lưới P2P.', folder: 'network' },
    { id: 'kql', name: 'KQL Query Language', code: 'P3', badgeClass: 'badge-nearly', progress: 'Gần hoàn thiện 90%', hasPaper: true, desc: 'Ngôn ngữ truy vấn khai báo riêng cho Đồ thị Tri thức, sử dụng nom-based recursive descent parser. Bao gồm local executor và distributed query engine tích hợp 6 cấp độ leo thang phạm vi SCOPE và 3 công cụ tự động phát hiện tri thức mới (Gap, Bridge, Serendipity).', folder: 'kql' },
    { id: 'pok', name: 'PoMV Consensus', code: 'P4', badgeClass: 'badge-nearly', progress: 'Gần hoàn thiện 85%', hasPaper: true, desc: 'Cơ chế đồng thuận Proof-of-Metabolic-Value độc đáo dựa trên quan sát phi tập trung, đo lường sự đóng góp tri thức qua 6 tín hiệu: Metabolism, Entropy, Prediction, Survival, Synaptic, và Niche. Hỗ trợ hệ thống miễn dịch chống tấn công Sybil mà không kiểm duyệt nội dung.', folder: 'pok' },
    { id: 'obt', name: 'Token Economics (OBT)', code: 'P5', badgeClass: 'badge-nearly', progress: 'Gần hoàn thiện 75%', hasPaper: true, desc: 'Hệ thống Token utility OBT dựa trên sổ cái Account-Chain (Nano-style), mint-on-demand để trả thưởng cho các hoạt động mã hóa tri thức, xác thực, và lưu trữ. Cơ chế Anti-Gaming ngăn chặn khai thác spam.', folder: 'obt' },
    { id: 'ai', name: 'AI Layer', code: 'P6', badgeClass: 'badge-research', progress: 'Đang nghiên cứu 30%', hasPaper: false, desc: 'Tích hợp các mô hình học máy (BERT-based, Sentence Transformers) nhằm hỗ trợ việc dịch và phân rã văn bản tự nhiên thành mã Core DNA, phát hiện tri thức trùng lặp, phân loại tri thức, và ánh xạ các liên kết tri thức tự động.' },
    { id: 'graph', name: 'Knowledge Graph', code: 'P7', badgeClass: 'badge-developing', progress: 'Đang phát triển 45%', hasPaper: false, desc: 'Đồ thị liên kết tri thức thông qua 33 loại liên kết hóa học-sinh học (bonds). Đã triển khai bộ khung synaptic bonds, thuật toán phát hiện lỗ hổng tri thức (Gap Detection), và công cụ bắc cầu tri thức Swanson ABC.' },
    { id: 'storage', name: 'Storage Layer', code: 'P8', badgeClass: 'badge-nearly', progress: 'Đã có nền tảng 65%', hasPaper: false, desc: 'Lưu trữ tri thức cục bộ bền vững tuân thủ ACID sử dụng cơ sở dữ liệu nhúng redb viết hoàn toàn bằng Rust. Dữ liệu được định danh nội dung bằng mã băm BLAKE3 (Content-Addressable). Dự kiến tích hợp lưu trữ phân tán IPFS trong tương lai.' },
    { id: 'bci', name: 'BCI Protocol', code: 'P9', badgeClass: 'badge-vision', progress: 'Tầm nhìn xa 15%', hasPaper: false, desc: 'Giao thức kết nối trực tiếp não bộ với máy tính (Brain-Computer Interface). Tầm nhìn Phase 5 (dự kiến 2030-2035) nhằm đồng bộ hóa tri thức trực tiếp từ xung thần kinh mà không cần qua nhập liệu bằng ngôn ngữ tự nhiên.' },
    { id: 'ui', name: 'User Interface', code: 'P10', badgeClass: 'badge-vision', progress: 'Chưa bắt đầu 10%', hasPaper: false, desc: 'Giao diện người dùng đồ họa để duyệt đồ thị tri thức, đóng góp KU, quản lý tài khoản token OBT và dashboard mạng lưới. Hiện tại chỉ có phiên bản CLI tương tác giả lập 3 node để chạy thử nghiệm.' }
  ];

  const chaptersData = [];
  const pillarsMetadata = {};

  for (const pillar of pillars) {
    if (!pillar.hasPaper) continue;

    const pillarFolder = path.join(paperViDir, pillar.folder);
    if (!fs.existsSync(pillarFolder)) {
      logFail(`Pillar folder not found: ${pillarFolder}`);
      continue;
    }
    const files = fs.readdirSync(pillarFolder)
      .filter(f => f.endsWith('.md'))
      .sort();
    
    log(`  📂 Loading ${files.length} chapters for ${pillar.name}...`);
    
    pillarsMetadata[pillar.id] = [];
    
    for (const file of files) {
      const filePath = path.join(pillarFolder, file);
      let mdContent = fs.readFileSync(filePath, 'utf-8');
      const fileId = `${pillar.id}_${file.replace('.md', '')}`;
      
      mdContent = processMarkdown(mdContent, fileId, tempDir);
      const htmlContent = marked.parse(mdContent);
      
      let chapName = file.replace('.md', '').replace(/_/g, ' ');
      chapName = chapName.charAt(0).toUpperCase() + chapName.slice(1);
      
      chaptersData.push({
        id: fileId,
        pillarId: pillar.id,
        html: htmlContent
      });

      pillarsMetadata[pillar.id].push({
        id: fileId,
        name: chapName
      });
    }
  }

  // 3. Construct Pillars cards HTML
  let pillarsCardsHtml = '';
  for (const pillar of pillars) {
    pillarsCardsHtml += `
      <article class="pillar-card" id="card-${pillar.id}">
        <div class="pillar-header">
          <div class="pillar-title-group">
            <span class="pillar-number">${pillar.code}</span>
            <h3 class="pillar-title">${pillar.name}</h3>
          </div>
          <span class="badge-status ${pillar.badgeClass}">${pillar.progress}</span>
        </div>
        <p class="pillar-description">${pillar.desc}</p>
    `;

    if (pillar.hasPaper && pillarsMetadata[pillar.id]) {
      pillarsCardsHtml += `
        <div class="paper-section">
          <h4 class="paper-section-title">
            <svg style="width:16px;height:16px;fill:var(--primary);" viewBox="0 0 24 24"><path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-1 17.93c-3.95-.49-7-3.85-7-7.93 0-.62.08-1.21.21-1.79L9 15v1c0 1.1.9 2 2 2v1.93zm6.9-2.54c-.26-.81-1-1.39-1.9-1.39h-1v-3c0-.55-.45-1-1-1H8v-2h2c.55 0 1-.45 1-1V7h2c1.1 0 2-.9 2-2v-.41c2.93 1.19 5 4.06 5 7.41 0 2.08-.8 3.97-2.1 5.39z"/></svg>
            Tài liệu Nghiên cứu Học thuật (Tiếng Việt)
          </h4>
          <div class="paper-path-flow">
            <span class="path-root">OneBrain</span>
            <span class="path-separator">➔</span>
            <span class="path-pillar-name">${pillar.id.toUpperCase()}</span>
            <span class="path-separator">➔</span>
      `;

      pillarsMetadata[pillar.id].forEach(chap => {
        pillarsCardsHtml += `
          <a class="path-link" onclick="openReader('${pillar.id}', '${chap.id}')">${chap.name}</a>
          <span class="path-separator">➔</span>
        `;
      });
      // Remove trailing separator
      pillarsCardsHtml = pillarsCardsHtml.substring(0, pillarsCardsHtml.lastIndexOf('<span class="path-separator">➔</span>'));

      pillarsCardsHtml += `
          </div>
        </div>
      `;
    }

    pillarsCardsHtml += `</article>`;
  }

  // 4. Construct Content bodies for reader panes
  let contentPanesHtml = '';
  for (const chap of chaptersData) {
    contentPanesHtml += `
      <div class="chapter-pane markdown-body" id="pane-${chap.id}">
        ${chap.html}
      </div>
    `;
  }

  // 5. Construct final HTML content
  const finalHtml = `<!DOCTYPE html>
<html lang="vi">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>OneBrain Portal - Mạng lưới Tri thức Phi tập trung</title>
  <meta name="description" content="Trang thông tin tổng quan, lộ trình phát triển và các tài liệu đặc tả học thuật tiếng Việt của 10 trụ cột chính dự án OneBrain.">
  <link rel="preconnect" href="https://fonts.googleapis.com">
  <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
  <link href="https://fonts.googleapis.com/css2?family=Inter:wght@300;400;500;600;700&family=Outfit:wght@400;500;600;700;800&family=JetBrains+Mono:wght@400;500&display=swap" rel="stylesheet">
  <style>
    ${LIGHT_CSS}
  </style>
</head>
<body>

  <!-- Landing Page Container -->
  <div class="container" id="landing-page">
    
    <!-- Hero Header -->
    <header class="hero">
      <h1>Hệ sinh thái OneBrain</h1>
      <p>
        Cơ sở hạ tầng chia sẻ và quản lý tri thức nhân loại phi tập trung lấy cảm hứng từ chuyển hóa sinh học.
      </p>
      
      <!-- Stats Grid -->
      <div class="project-meta">
        <div class="meta-card">
          <div class="label">Quy mô Codebase</div>
          <div class="value">~38,500 LOC Rust</div>
        </div>
        <div class="meta-card">
          <div class="label">Hệ thống Đánh giá</div>
          <div class="value">707 Tests</div>
        </div>
        <div class="meta-card">
          <div class="label">Tài liệu Nghiên cứu</div>
          <div class="value">5 Trụ cột hoàn tất</div>
        </div>
      </div>
    </header>

    <!-- Overview section from README - Part 1: Intro -->
    <section class="overview-section" style="margin-bottom: 4rem;">
      <h2 class="section-title">Tổng quan Hệ thống</h2>
      <div class="markdown-body">
        ${readmeHtmlPart1}
      </div>
    </section>

    <!-- 10 Pillars grid -->
    <section id="pillars" style="margin-bottom: 4rem;">
      <h2 class="section-title" style="margin-bottom: 0.25rem;">Tài liệu Học thuật (Paper Document)</h2>
      <h3 style="font-family: var(--font-heading); font-size: 1.25rem; font-weight: 500; color: var(--text-muted); margin-bottom: 2rem;">10 Trụ cột Công nghệ (Gồm 5 tài liệu nghiên cứu đã hoàn thành)</h3>
      <div class="pillars-grid">
        ${pillarsCardsHtml}
      </div>
    </section>

    <!-- Overview section from README - Part 2: Quick Start & Specs -->
    <section class="overview-section" style="margin-bottom: 4rem;">
      <div class="markdown-body">
        ${readmeHtmlPart2}
      </div>
    </section>

    <!-- Footer -->
    <footer>
      <p>© 2026 Dự án <span style="font-weight:600;color:var(--primary-hover);">OneBrain</span> Contributors. Tất cả các quyền được bảo lưu.</p>
      <p>Địa chỉ email liên hệ: <a href="mailto:shpy2001@gmail.com" style="color:var(--primary);text-decoration:none;font-weight:600;">shpy2001@gmail.com</a></p>
    </footer>

  </div>

  <!-- Fullscreen Reader Overlay (SPA) -->
  <div class="reader-overlay" id="reader-overlay">
    
    <!-- Reader Sidebar -->
    <aside class="reader-sidebar">
      <div class="reader-sidebar-header">
        <button class="btn-close" onclick="closeReader()">
          ⬅ Quay lại Landing Page
        </button>
        <div class="reader-sidebar-title-row">
          <div class="reader-pillar-name" id="reader-pillar-title">Pillar Title</div>
          <button class="btn-menu-toggle" id="btn-menu-toggle" onclick="toggleMobileMenu()">
            📖 Hiện danh sách
          </button>
        </div>
      </div>
      <nav class="reader-menu" id="reader-menu-list">
        <!-- Dyn populated via JS -->
      </nav>
    </aside>

    <!-- Reader Main Body -->
    <main class="reader-main">
      <div class="reader-top-bar">
        <div class="reader-breadcrumb">
          <span class="reader-bc-current" id="reader-breadcrumb-chapter">Chapter Name</span>
        </div>
      </div>
      <div class="reader-content-scroll" id="reader-content-scroll">
        <div class="reader-content-body">
          ${contentPanesHtml}
        </div>
      </div>
    </main>

  </div>

  <!-- JS Data & Control Routing -->
  <script>
    const PILLARS_DATA = ${JSON.stringify(pillarsMetadata)};
    const PILLAR_NAMES = {
      ku: 'P1: Knowledge Unit (KU)',
      network: 'P2: Network Protocol (OBP)',
      kql: 'P3: KQL Query Language',
      pok: 'P4: PoMV Consensus',
      obt: 'P5: Token Economics (OBT)'
    };
    
    let currentPillar = '';
    let currentChapter = '';

    function toggleMobileMenu() {
      const menu = document.getElementById('reader-menu-list');
      const btn = document.getElementById('btn-menu-toggle');
      if (menu.classList.contains('expanded')) {
        menu.classList.remove('expanded');
        btn.innerHTML = '📖 Hiện danh sách';
      } else {
        menu.classList.add('expanded');
        btn.innerHTML = '📖 Ẩn danh sách';
      }
    }

    function openReader(pillarId, chapterId) {
      currentPillar = pillarId;
      currentChapter = chapterId;
      
      // Auto collapse mobile menu
      const menuEl = document.getElementById('reader-menu-list');
      menuEl.classList.remove('expanded');
      const toggleBtn = document.getElementById('btn-menu-toggle');
      if (toggleBtn) toggleBtn.innerHTML = '📖 Hiện danh sách';
      
      // Populate sidebar list
      menuEl.innerHTML = '';
      
      const chapters = PILLARS_DATA[pillarId];
      if (chapters) {
        chapters.forEach(chap => {
          const item = document.createElement('div');
          item.className = 'reader-menu-item' + (chap.id === chapterId ? ' active' : '');
          item.innerText = chap.name;
          item.onclick = () => loadChapter(pillarId, chap.id);
          item.id = 'menu-' + chap.id;
          menuEl.appendChild(item);
        });
      }
      
      // Update header
      document.getElementById('reader-pillar-title').innerText = PILLAR_NAMES[pillarId];
      
      // Show overlay, block page scrolling
      document.getElementById('reader-overlay').classList.add('active');
      document.body.style.overflow = 'hidden';
      
      // Render chapter content
      showChapterContent(chapterId);
    }

    function loadChapter(pillarId, chapterId) {
      currentChapter = chapterId;
      
      // Auto collapse mobile menu
      const menu = document.getElementById('reader-menu-list');
      menu.classList.remove('expanded');
      const toggleBtn = document.getElementById('btn-menu-toggle');
      if (toggleBtn) toggleBtn.innerHTML = '📖 Hiện danh sách';
      
      // Toggle sidebar active states
      document.querySelectorAll('.reader-menu-item').forEach(el => el.classList.remove('active'));
      const activeEl = document.getElementById('menu-' + chapterId);
      if (activeEl) activeEl.classList.add('active');
      
      showChapterContent(chapterId);
    }

    function showChapterContent(chapterId) {
      // Hide all panes
      document.querySelectorAll('.chapter-pane').forEach(el => el.classList.remove('active'));
      
      // Show selected pane
      const pane = document.getElementById('pane-' + chapterId);
      if (pane) {
        pane.classList.add('active');
      }
      
      // Set breadcrumb chapter name
      const chapterMetadata = PILLARS_DATA[currentPillar].find(c => c.id === chapterId);
      if (chapterMetadata) {
        document.getElementById('reader-breadcrumb-chapter').innerText = chapterMetadata.name;
      }
      
      // Scroll read section back to top
      document.getElementById('reader-content-scroll').scrollTop = 0;
    }

    function closeReader() {
      document.getElementById('reader-overlay').classList.remove('active');
      document.body.style.overflow = 'auto';
    }
  </script>

</body>
</html>
`;

  // Write out the compiled file
  fs.writeFileSync(outputPath, finalHtml, 'utf-8');
  
  // Clean up temp dir
  log('  清理临时文件...');
  fs.rmSync(tempDir, { recursive: true, force: true });
  
  const sizeMb = (fs.statSync(outputPath).size / (1024 * 1024)).toFixed(2);
  logOk(`Compiled successfully! Landing page created at: ${outputPath} (${sizeMb} MB)`);
}

main();
