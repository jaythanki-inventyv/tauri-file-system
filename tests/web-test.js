const { chromium } = require('playwright');
const fs = require('fs');
const path = require('path');

const APP = 'http://localhost:1420/';
const OUT = __dirname;

// --- generate test files ---
const PNG_1PX = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGNiYPj/HwADhgGAWjR9awAAAABJRU5ErkJggg==',
  'base64'
);
const PDF_MIN = Buffer.from(
  '%PDF-1.4\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n' +
  '2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj\n' +
  '3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 200 200]>>endobj\n' +
  'xref\n0 4\n0000000000 65535 f \ntrailer<</Size 4/Root 1 0 R>>\nstartxref\n0\n%%EOF\n'
);

const files = {};
for (const n of ['a.png', 'b.png', 'c.png']) {
  files[n] = path.join(OUT, n);
  fs.writeFileSync(files[n], PNG_1PX);
}
for (const n of ['doc1.pdf', 'doc2.pdf']) {
  files[n] = path.join(OUT, n);
  fs.writeFileSync(files[n], PDF_MIN);
}

const results = [];
const ok = (name, pass, detail = '') =>
  results.push(`${pass ? 'PASS' : 'FAIL'}  ${name}${detail ? ' — ' + detail : ''}`);

(async () => {
  const browser = await chromium.launch();
  const ctx = await browser.newContext({ acceptDownloads: true });
  const page = await ctx.newPage();
  const consoleErrors = [];
  page.on('console', (m) => m.type() === 'error' && consoleErrors.push(m.text()));
  page.on('pageerror', (e) => consoleErrors.push(String(e)));

  await page.goto(APP, { waitUntil: 'networkidle' });
  ok('page loads', await page.locator('h1').count() === 1, await page.locator('h1').textContent());

  const inputs = page.locator('input[type=file]');
  ok('3 pickers present', (await inputs.count()) === 3);

  // 1. multiple images
  await inputs.nth(0).setInputFiles([files['a.png'], files['b.png'], files['c.png']]);
  await page.waitForTimeout(300);
  const status1 = await page.locator('.status').textContent();
  ok('multi-image pick → status', status1.includes('3'), status1.trim());
  ok('3 cards render', (await page.locator('.card').count()) === 3);
  ok('img previews render', (await page.locator('.card img').count()) === 3);

  // 2. multiple documents (replaces selection)
  await inputs.nth(1).setInputFiles([files['doc1.pdf'], files['doc2.pdf']]);
  await page.waitForTimeout(300);
  ok('multi-pdf pick → 2 cards', (await page.locator('.card').count()) === 2);
  const meta = await page.locator('.card .meta').first().textContent();
  ok('metadata shows size+mime', /B.*application\/pdf/.test(meta), meta.trim());

  // 3. empty change event must NOT wipe selection
  await inputs.nth(1).evaluate((el) => {
    el.value = '';
    el.dispatchEvent(new Event('change', { bubbles: true }));
  });
  await page.waitForTimeout(200);
  ok('empty change keeps selection', (await page.locator('.card').count()) === 2,
     (await page.locator('.status').textContent()).trim());

  // 4. preview overlay (PDF → iframe)
  await page.locator('.card').first().click();
  await page.waitForTimeout(300);
  ok('overlay opens', await page.locator('.overlay').isVisible());
  ok('pdf iframe present', (await page.locator('.overlay iframe').count()) === 1);
  await page.screenshot({ path: path.join(OUT, 'shot-preview.png') });
  await page.locator('.overlay').click({ position: { x: 10, y: 10 } });
  await page.waitForTimeout(200);
  ok('overlay closes', (await page.locator('.overlay').count()) === 0);

  // 5. single save (💾) → browser download
  const dl1 = page.waitForEvent('download', { timeout: 5000 });
  await page.locator('.save-btn').first().click();
  try {
    const d = await dl1;
    ok('💾 single download fires', true, d.suggestedFilename());
  } catch { ok('💾 single download fires', false, 'no download event'); }
  ok('save did not open preview', (await page.locator('.overlay').count()) === 0);

  // 6. Download All → ONE zip (Safari allows a single download per click)
  const got = [];
  page.on('download', (d) => got.push(d.suggestedFilename()));
  await page.locator('.dl-all').click();
  await page.waitForTimeout(3000);
  ok('Download All → single zip', got.length === 1 && got[0] === 'files.zip', `got: ${got.join(', ')}`);

  // 7. re-picking same file works (input.value reset)
  await inputs.nth(0).setInputFiles([files['a.png']]);
  await page.waitForTimeout(200);
  await inputs.nth(0).setInputFiles([files['a.png']]);
  await page.waitForTimeout(200);
  ok('re-pick same file works', (await page.locator('.card').count()) === 1,
     (await page.locator('.status').textContent()).trim());

  await page.screenshot({ path: path.join(OUT, 'shot-final.png') });
  ok('no console errors', consoleErrors.length === 0, consoleErrors.slice(0, 3).join(' | '));

  console.log(results.join('\n'));
  await browser.close();
})().catch((e) => { console.error('SCRIPT ERROR:', e); process.exit(1); });
