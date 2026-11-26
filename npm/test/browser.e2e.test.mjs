// Loads the browser entry in Chromium the way an app does: native ES modules,
// the .wasm file fetched next to the bindings, under a strict Content Security Policy.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { chromium } from 'playwright';

const pkg = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const fixtures = path.resolve(pkg, '../fixtures');
const TYPES = { '.js': 'text/javascript', '.wasm': 'application/wasm', '.html': 'text/html' };
const CSP = "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'";

const PAGE = `<!doctype html><script type="module" src="/test/browser-page.js"></script>`;

function serve() {
  const server = createServer(async (req, res) => {
    const url = new URL(req.url, 'http://x');
    if (url.pathname === '/') {
      res.writeHead(200, { 'content-type': 'text/html', 'content-security-policy': CSP });
      return res.end(PAGE);
    }
    const base = url.pathname.startsWith('/fixtures/') ? path.dirname(fixtures) : pkg;
    const file = path.normalize(path.join(base, url.pathname));
    if (!file.startsWith(base)) return res.writeHead(403).end();
    try {
      const body = await readFile(file);
      res.writeHead(200, { 'content-type': TYPES[path.extname(file)] ?? 'application/octet-stream' });
      res.end(body);
    } catch {
      res.writeHead(404).end();
    }
  });
  return new Promise((resolve) => server.listen(0, '127.0.0.1', () => resolve(server)));
}

test('reads and writes in Chromium under a strict CSP', async () => {
  const server = await serve();
  const browser = await chromium.launch();
  try {
    const page = await browser.newPage();
    const errors = [];
    page.on('pageerror', (e) => errors.push(e.message));
    page.on('console', (m) => m.type() !== 'log' && errors.push(m.text()));
    await page.goto(`http://127.0.0.1:${server.address().port}/`);
    const result = await page.waitForFunction(() => window.__result, null, { timeout: 30_000 }).then((h) => h.jsonValue());
    assert.deepEqual(errors, []);
    assert.equal(result.error, undefined, result.error);
    assert.deepEqual(result.sheetNames, ['Daten', 'Zweite']);
    assert.deepEqual(result.row, ['Größe Äpfel ß €', 1234.56, '2024-02-29T00:00:00.000Z', true, 2469.12, null]);
    assert.deepEqual(result.roundTrip, [['a'], [1]]);
    assert.equal(result.errorCode, 'CORRUPT_FILE');
    assert.equal(result.trapCode, 'CORRUPT_FILE');
    assert.equal(result.afterTrap, 'Ende');
  } finally {
    await browser.close();
    server.close();
  }
});

test('a failed load is WASM_LOAD_FAILED and can be retried', async () => {
  const server = await serve();
  const browser = await chromium.launch();
  try {
    const page = await browser.newPage();
    await page.goto(`http://127.0.0.1:${server.address().port}/?loadfail`);
    const result = await page.waitForFunction(() => window.__result, null, { timeout: 30_000 }).then((h) => h.jsonValue());
    assert.equal(result.error, undefined, result.error);
    assert.equal(result.code, 'WASM_LOAD_FAILED');
    assert.deepEqual(result.rows, [['a', 'b']]);
  } finally {
    await browser.close();
    server.close();
  }
});
