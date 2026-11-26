// The browser entry (ES module, async API) exercised in Node.js with the
// WebAssembly bytes passed explicitly. Real-browser loading: browser.e2e.test.mjs.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { fixture, lib, plain } from './helpers.mjs';

const pkg = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const browser = await import(path.join(pkg, 'dist/browser/browser.js'));
const wasmBytes = readFileSync(path.join(pkg, 'wasm/web/excel_rs_wasm_bg.wasm'));

test('initExcel can be called repeatedly and concurrently', async () => {
  await Promise.all([browser.initExcel(wasmBytes), browser.initExcel(wasmBytes), browser.initExcel()]);
});

test('initExcel rejects a different source once loaded', async () => {
  await assert.rejects(browser.initExcel(new Uint8Array([1])), { code: 'INVALID_INPUT' });
});

test('reads the same rows as the Node.js entry', async () => {
  for (const file of ['reference-libreoffice.xlsx', 'reference-libreoffice.xls', 'reference-libreoffice.ods', 'german-excel.csv']) {
    const res = await browser.readSheet(fixture(file));
    assert.deepEqual(plain(res.rows), plain(lib.readSheet(fixture(file)).rows), file);
  }
});

test('write and read round trip', async () => {
  const sheets = [{ name: 'S', columns: [{ header: 'Datum', format: 'date' }], rows: [[new Date(Date.UTC(2024, 1, 29))]] }];
  const res = await browser.readSheet(await browser.writeXlsx(sheets));
  assert.deepEqual(plain(res.rows), [['Datum'], ['2024-02-29T00:00:00.000Z']]);
});

test('errors are ExcelError with codes, rejected as promises', async () => {
  await assert.rejects(browser.readSheet(Buffer.from([0, 1, 2])), { code: 'UNSUPPORTED_FORMAT' });
  await assert.rejects(browser.readSheet(fixture('utf8-bom.csv'), { sheet: -1 }), { code: 'INVALID_INPUT' });
  await assert.rejects(browser.writeXlsx([]), (err) => err instanceof browser.ExcelError && err.code === 'INVALID_INPUT');
});

test('recovers after a parser crash', async () => {
  await assert.rejects(browser.readSheet(fixture('reference-libreoffice.xls').subarray(0, 513)), { code: 'CORRUPT_FILE' });
  assert.equal((await browser.readSheet(fixture('reference-libreoffice.xls'))).rows[3][5], 'Ende');
});
