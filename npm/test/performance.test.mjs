import { test } from 'node:test';
import assert from 'node:assert/strict';
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
