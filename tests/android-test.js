const { _android } = require('playwright');
const { execSync } = require('child_process');
const fs = require('fs');
const path = require('path');

const PKG = 'com.purv.imagefiles.debug';
const ACTIVITY = 'com.purv.imagefiles.MainActivity';
const OUT = __dirname;

const results = [];
const ok = (name, pass, detail = '') =>
  results.push(`${pass ? 'PASS' : 'FAIL'}  ${name}${detail ? ' — ' + detail : ''}`);
const sh = (cmd) => execSync(cmd, { encoding: 'utf8' });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// Find a clickable node in the current native UI dump whose text matches `re`,
// return its center point.
function findNativeButton(re) {
  sh('adb shell uiautomator dump /sdcard/uidump.xml');
  const xml = sh('adb shell cat /sdcard/uidump.xml');
  const nodes = xml.split('<node ').slice(1);
  for (const n of nodes) {
    const text = (n.match(/text="([^"]*)"/) || [])[1] || '';
    const desc = (n.match(/content-desc="([^"]*)"/) || [])[1] || '';
    if (re.test(text) || re.test(desc)) {
      const b = n.match(/bounds="\[(\d+),(\d+)\]\[(\d+),(\d+)\]"/);
      if (b) return { x: (+b[1] + +b[3]) / 2, y: (+b[2] + +b[4]) / 2, label: text || desc };
    }
  }
  return null;
}

(async () => {
  // Fresh app start
  sh(`adb shell am force-stop ${PKG}`);
  await sleep(800);
  sh(`adb shell am start -n ${PKG}/${ACTIVITY}`);
  await sleep(2500);

  const [device] = await _android.devices();
  if (!device) throw new Error('no adb device');
  console.log(`device: ${device.serial()} (${device.model()})`);

  const webview = await device.webView({ pkg: PKG }, { timeout: 15000 });
  const page = await webview.page();
  ok('attached to app webview', true, page.url());

  await page.waitForSelector('h1', { timeout: 10000 });
  ok('app UI loaded', true, (await page.locator('h1').textContent()).trim());

  // --- inject a multi-file pick directly into the real <input> (no picker UI,
  // but the exact same change-event path the app uses) ---
  const pickResult = await page.evaluate(() => {
    const png = Uint8Array.from(atob(
      'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGNiYPj/HwADhgGAWjR9awAAAABJRU5ErkJggg=='
    ), (c) => c.charCodeAt(0));
    const pdfBytes = new TextEncoder().encode(
      '%PDF-1.4\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj\n3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 200 200]>>endobj\ntrailer<</Size 4/Root 1 0 R>>\n%%EOF\n'
    );
    const dt = new DataTransfer();
    dt.items.add(new File([png], 'test-a.png', { type: 'image/png' }));
    dt.items.add(new File([png], 'test-b.png', { type: 'image/png' }));
    dt.items.add(new File([pdfBytes], 'test-doc.pdf', { type: 'application/pdf' }));
    const input = document.querySelector('input[type=file]');
    input.files = dt.files;
    input.dispatchEvent(new Event('change', { bubbles: true }));
    return input.files.length;
  });
  await sleep(600);
  ok('injected 3-file pick', pickResult === 3);

  const status = (await page.locator('.status').textContent()).trim();
  ok('status shows 3 loaded', status.includes('3'), status);
  ok('3 cards render on device', (await page.locator('.card').count()) === 3);
  ok('image previews render', (await page.locator('.card img').count()) === 2);

  // --- preview overlay on device ---
  await page.locator('.card').nth(2).click(); // the PDF
  await sleep(600);
  ok('overlay opens (pdf iframe)', (await page.locator('.overlay iframe').count()) === 1);
  sh(`adb exec-out screencap -p > "${path.join(OUT, 'android-preview.png')}"`);
  await page.locator('.overlay').click({ position: { x: 20, y: 40 } });
  await sleep(400);
  ok('overlay closes', (await page.locator('.overlay').count()) === 0);

  // --- SAVE: tap 💾 → native SAF "Save As" should open ---
  sh(`adb shell rm -f /sdcard/Download/test-a.png`);
  await page.locator('.save-btn').first().click();
  await sleep(3000);

  sh(`adb exec-out screencap -p > "${path.join(OUT, 'android-saf.png')}"`);
  const saveBtn = findNativeButton(/^(save|સાચવો|सहेजें)$/i);
  ok('SAF Save-As dialog opened', !!saveBtn, saveBtn ? `native button: "${saveBtn.label}"` : 'no Save button in native UI');

  if (saveBtn) {
    sh(`adb shell input tap ${Math.round(saveBtn.x)} ${Math.round(saveBtn.y)}`);
    await sleep(3000);
    const st = (await page.locator('.status').textContent()).trim();
    ok('status shows saved', st.startsWith('✅'), st);
    const ls = sh('adb shell ls /sdcard/Download/ 2>/dev/null || true');
    ok('file visible in Downloads', /test-a\.png/.test(ls),
      /test-a\.png/.test(ls) ? 'found /sdcard/Download/test-a.png' : 'not in Downloads (user may have picked another folder)');
  }

  sh(`adb exec-out screencap -p > "${path.join(OUT, 'android-final.png')}"`);
  console.log(results.join('\n'));
  await page.close().catch(() => { });
})().catch((e) => {
  console.log(results.join('\n'));
  console.error('SCRIPT ERROR:', e.message);
  process.exit(1);
});
