// Start a temporary Vite server on 127.0.0.1:5197 before this check.
// Uses the repository's existing Puppeteer dependency and isolated fixtures only.
const puppeteer = require('puppeteer');
const assert = require('node:assert/strict');
const path = require('node:path');
const fs = require('node:fs');
(async () => {
  const browser = await puppeteer.launch({ headless: true });
  try {
    const page = await browser.newPage();
    const errors = [];
    page.on('pageerror', e => errors.push(e.message));
    await page.setRequestInterception(true);
    page.on('request', request => {
      const url = new URL(request.url());
      // No external fonts, live API, analytics or peer traffic during visual QA.
      if (url.origin === 'http://127.0.0.1:5197' || url.protocol === 'data:') request.continue();
      else request.abort();
    });
    const output = process.env.OBP_SCREENSHOT_DIR;
    if (output) fs.mkdirSync(output, { recursive: true });
    for (const width of [1280, 390]) {
      await page.setViewport({ width, height: 1000, deviceScaleFactor: 1 });
      await page.goto('http://127.0.0.1:5197/tests/obp-browser.html', { waitUntil: 'networkidle0' });
      await page.waitForFunction(() => document.body.textContent.includes('Session and durable network generation'));
      const layout = await page.evaluate(() => {
        const grid = document.querySelector('.obp-grid');
        const pane = document.querySelector('.obp-network');
        return { columns: getComputedStyle(grid).gridTemplateColumns.split(' ').length, overflow: pane.scrollWidth > pane.clientWidth, labels: [...document.querySelectorAll('input[id],select[id]')].every(el => !!document.querySelector(`label[for="${el.id}"]`)) };
      });
      assert.equal(layout.columns, width < 760 ? 1 : 2);
      assert.equal(layout.overflow, false); assert.equal(layout.labels, true);
      await page.addScriptTag({ path: require.resolve('axe-core/axe.min.js') });
      const violations = await page.evaluate(async () => (await axe.run()).violations.map(v => ({ id: v.id, nodes: v.nodes.length })));
      assert.deepEqual(violations, []);
      if (output) await page.screenshot({ path: path.join(output, `obp-${width}.png`), fullPage: true });
      await page.evaluate(() => document.querySelector('#obp-management').scrollIntoView());
      if (output) await page.screenshot({ path: path.join(output, `obp-management-${width}.png`), fullPage: true });
      console.log(`OBP ${width}px: responsive columns, no horizontal overflow, labelled inputs, axe including contrast PASS`);
    }
    assert.deepEqual(errors, []);
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
