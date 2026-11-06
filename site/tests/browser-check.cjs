// Run with Playwright available through NODE_PATH; no package/build dependency.
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const http = require('node:http');
const path = require('node:path');
const { chromium } = require('playwright');

(async () => {
  const root = path.resolve(__dirname, '..');
  const types = { '.html': 'text/html', '.css': 'text/css', '.js': 'text/javascript', '.svg': 'image/svg+xml', '.png': 'image/png' };
  const server = http.createServer(async (req, res) => {
    const pathname = decodeURIComponent(new URL(req.url, 'http://localhost').pathname);
    const file = path.resolve(root, '.' + pathname, pathname.endsWith('/') ? 'index.html' : '');
    if (!file.startsWith(root + path.sep)) { res.writeHead(404).end(); return; }
    try {
      const bytes = await fs.readFile(file);
      res.setHeader('Content-Type', types[path.extname(file)] || 'text/plain');
      res.end(bytes);
    } catch { res.writeHead(404).end(); }
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  let browser;
  try {
    browser = await chromium.launch({ headless: true, executablePath: process.env.SHEPRD_TEST_CHROMIUM });
    const origin = `http://127.0.0.1:${server.address().port}`;
    for (const width of [1440, 390, 320]) {
      const page = await browser.newPage({ viewport: { width, height: 900 }, colorScheme: 'dark' });
      const errors = [];
      page.on('pageerror', e => errors.push(e.message));
      await page.route('https://api.github.com/**', route => {
        assert.equal(route.request().url(), 'https://api.github.com/repos/sheprd-labs/sheprd');
        return route.fulfill({ json: { stargazers_count: 1234 } });
      });
      await page.goto(origin, { waitUntil: 'networkidle' });
      assert.equal(await page.title(), 'Sheprd: a native workspace for coding agents');
      assert.equal(await page.locator('#stars-n').innerText(), '1.2k');
      assert.ok(await page.locator('.row').count() > 0);
      assert.equal(await page.locator('img').evaluateAll(images => images.every(img => img.complete && img.naturalWidth > 0)), true);
      assert.equal(await page.locator('#film').isVisible(), false);
      assert.equal(await page.locator('#shots').isVisible(), false);
      for (const os of ['linux', 'windows', 'macos']) {
        await page.locator(`#tab-${os}`).click();
        assert.equal(await page.locator(`#os-${os}`).isVisible(), true);
      }
      await page.locator('#tab-macos').focus();
      await page.keyboard.press('ArrowRight');
      assert.equal(await page.locator('#tab-linux').getAttribute('aria-selected'), 'true');
      assert.equal(await page.locator('#os-linux').isVisible(), true);
      await page.evaluate(() => {
        window.copied = '';
        Object.defineProperty(navigator, 'clipboard', { value: { writeText: async text => { window.copied = text; } } });
      });
      await page.locator('#os-linux .copy').click();
      assert.equal(await page.evaluate(() => window.copied), 'curl -fsSL https://sheprd.org/install.sh | sh');
      assert.equal(await page.locator('#os-linux .copy').innerText(), 'Copied');
      assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
      for (const theme of ['light', 'dark']) {
        await page.locator('#theme').click();
        assert.equal(await page.locator('html').getAttribute('data-theme'), theme);
        if (process.env.SHEPRD_SITE_SCREENSHOTS) {
          await page.screenshot({ path: path.join(process.env.SHEPRD_SITE_SCREENSHOTS, `home-${width}-${theme}.png`), fullPage: true });
        }
      }
      await page.goto(origin + '/names/', { waitUntil: 'networkidle' });
      assert.equal(await page.locator('h1').innerText(), 'Meet Sheprd.');
      assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
      assert.equal(await page.locator('img').evaluateAll(images => images.every(img => img.complete && img.naturalWidth > 0)), true);
      if (process.env.SHEPRD_SITE_SCREENSHOTS) {
        await page.screenshot({ path: path.join(process.env.SHEPRD_SITE_SCREENSHOTS, `names-${width}.png`), fullPage: true });
      }
      assert.deepEqual(errors, []);
      await page.close();
      console.log(`PASS ${width}px: theme, tabs, keyboard, copy, assets, announcement, no overflow/runtime errors`);
    }
  } finally {
    if (browser) await browser.close();
    await new Promise(resolve => server.close(resolve));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
