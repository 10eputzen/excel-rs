import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { gzipSync } from 'node:zlib';
import { lib } from './helpers.mjs';

const { readSheet, writeXlsx } = lib;

// Budget for 100,000 rows × 10 columns. Typical: well under 1 s; the CI limit
// leaves headroom for slow shared runners.
const BUDGET_MS = Number(process.env.EXCEL_RS_PERF_BUDGET_MS ?? 3000);

test(`imports 100,000 rows within ${BUDGET_MS} ms`, () => {
  const rows = Array.from({ length: 100_000 }, (_, i) => [
    `Name ${i}`, i, i * 1.5, i % 2 === 0, new Date(Date.UTC(2024, 0, 1 + (i % 365))),
    'Straße', i % 7, `x${i}`, -i, 'ende',
  ]);
  const xlsx = writeXlsx([{ name: 'S', columns: [], rows }]);
  const start = performance.now();
  const res = readSheet(xlsx);
  const ms = performance.now() - start;
  assert.equal(res.rows.length, 100_000);
  assert.equal(res.rows[99_999][1], 99_999);
  console.log(`# xlsx import 100k rows: ${ms.toFixed(0)} ms`);
  assert.ok(ms < BUDGET_MS, `took ${ms.toFixed(0)} ms`);

  const csv = Buffer.from(rows.map((r) => r.map((v) => (v instanceof Date ? v.toISOString() : v)).join(';')).join('\n'));
  const t2 = performance.now();
  assert.equal(readSheet(csv).rows.length, 100_000);
  const csvMs = performance.now() - t2;
  console.log(`# csv import 100k rows: ${csvMs.toFixed(0)} ms`);
  assert.ok(csvMs < BUDGET_MS, `csv took ${csvMs.toFixed(0)} ms`);
});

test(`writes 100,000 rows within ${BUDGET_MS} ms`, () => {
  // Was 11 s before the heap was reserved up front: every growth of the
  // WebAssembly memory triggered a full garbage collection in V8.
  const rows = Array.from({ length: 100_000 }, (_, i) => [
    `Kunde ${i}`, i, i * 1.25, i % 2 === 0, new Date(Date.UTC(2024, 0, 1 + (i % 365))),
    'Müllerstraße 12', i % 97, `Bestellung ${i}; Lieferung`, -i / 3, 'Köln',
  ]);
  writeXlsx([{ name: 'S', columns: [], rows: rows.slice(0, 1000) }]); // warm-up
  const start = performance.now();
  writeXlsx([{ name: 'S', columns: [], rows }]);
  const ms = performance.now() - start;
  console.log(`# xlsx export 100k rows: ${ms.toFixed(0)} ms`);
  assert.ok(ms < BUDGET_MS, `took ${ms.toFixed(0)} ms`);
});

// Fixed limits against regressions: excel-rs must stay clearly faster than
// SheetJS on the same file, and the WebAssembly binary must not grow unnoticed.
test('reads xlsx at least 2× faster than SheetJS', () => {
  const XLSX = createRequire(import.meta.url)('xlsx');
  const rows = Array.from({ length: 50_000 }, (_, i) => [`Kunde ${i}`, i, i * 1.25, i % 2 === 0, 'Müllerstraße 12', `x${i}`]);
  const wb = XLSX.utils.book_new();
  XLSX.utils.book_append_sheet(wb, XLSX.utils.aoa_to_sheet(rows), 'S');
  const bytes = XLSX.write(wb, { type: 'buffer', bookType: 'xlsx' });
  const best = (fn) => Math.min(...[0, 1, 2].map(() => { const t = performance.now(); fn(); return performance.now() - t; }));
  const ours = best(() => readSheet(bytes));
  const theirs = best(() => XLSX.utils.sheet_to_json(XLSX.read(bytes).Sheets.S, { header: 1, raw: true }));
  console.log(`# 50k rows: excel-rs ${ours.toFixed(0)} ms, SheetJS ${theirs.toFixed(0)} ms (${(theirs / ours).toFixed(1)}×)`);
  assert.ok(ours * 2 < theirs, `excel-rs ${ours.toFixed(0)} ms vs SheetJS ${theirs.toFixed(0)} ms`);
});

test('WebAssembly binary stays under 900 KB gzip', () => {
  const wasm = readFileSync(new URL('../wasm/node/excel_rs_wasm_bg.wasm', import.meta.url));
  const kb = gzipSync(wasm, { level: 9 }).length / 1024;
  console.log(`# wasm: ${(wasm.length / 1024).toFixed(0)} KB, ${kb.toFixed(0)} KB gzip`);
  assert.ok(kb < 900, `${kb.toFixed(0)} KB gzip`);
});
