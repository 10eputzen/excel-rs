import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync } from 'node:fs';
import { lib, fixture, fixturesDir } from './helpers.mjs';

const { readSheet, writeXlsx, ExcelError } = lib;
const files = readdirSync(fixturesDir);

/** Reading must either succeed or throw an ExcelError, never anything else. */
const readSafely = (bytes) => {
  try {
    readSheet(bytes);
  } catch (err) {
    if (!(err instanceof ExcelError)) throw err;
  }
};

test('truncated files fail cleanly', () => {
  for (const file of files) {
    const bytes = fixture(file);
    for (const frac of [0.1, 0.3, 0.5, 0.7, 0.9, 0.99]) {
      readSafely(bytes.subarray(0, Math.floor(bytes.length * frac)));
    }
  }
});

test('corrupted files fail cleanly', () => {
  let seed = 7;
  const rand = () => ((seed = (seed * 1103515245 + 12345) % 2 ** 31) / 2 ** 31);
  for (const file of files) {
    const original = fixture(file);
    for (let round = 0; round < 30; round++) {
      const bytes = Buffer.from(original);
      for (let i = 0; i < 8; i++) bytes[Math.floor(rand() * bytes.length)] = Math.floor(rand() * 256);
      readSafely(bytes);
    }
  }
  // The module still works after all the broken input.
  assert.deepEqual(readSheet(fixture('utf8-bom.csv')).rows[0], ['a', 'b']);
});

test('a parser crash (WebAssembly trap) becomes CORRUPT_FILE and the module recovers', () => {
  // This prefix of the xls fixture makes the parser panic, which aborts the WASM instance.
  const xls = fixture('reference-libreoffice.xls').subarray(0, 513);
  assert.throws(() => readSheet(xls), { code: 'CORRUPT_FILE', message: /internal error/ });
  assert.equal(readSheet(fixture('reference-libreoffice.xls')).rows[3][5], 'Ende');
  assert.ok(writeXlsx([{ name: 'S', columns: [], rows: [[1]] }]).length > 0);
});

test('size limits', () => {
  const xlsx = fixture('reference-libreoffice.xlsx');
  assert.throws(() => readSheet(xlsx, { maxBytes: 100 }), { code: 'TOO_LARGE' });
  assert.throws(() => readSheet(xlsx, { maxUncompressedBytes: 1000 }), { code: 'TOO_LARGE' });
  assert.throws(() => readSheet(xlsx, { maxRows: 3 }), { code: 'TOO_LARGE' });
  assert.throws(() => readSheet(xlsx, { maxCells: 23 }), { code: 'TOO_LARGE' });
  assert.equal(readSheet(xlsx, { maxRows: 4, maxCells: 24 }).rows.length, 4);
  assert.throws(() => readSheet(Buffer.from('1\n2\n3\n'), { maxRows: 2 }), { code: 'TOO_LARGE' });
  assert.throws(() => readSheet(xlsx, { maxRows: -1 }), { code: 'INVALID_INPUT' });
  // Large limits are not truncated to 32 bits.
  assert.equal(readSheet(xlsx, { maxBytes: 2 ** 32 }).rows.length, 4);
});

test('highly compressed archive (zip bomb) is rejected before parsing', () => {
  const rows = Array.from({ length: 50_000 }, () => ['aaaaaaaaaaaaaaaaaaaa', 0]);
  const bomb = writeXlsx([{ name: 'S', columns: [], rows }]);
  assert.ok(bomb.length < 1_000_000);
  assert.throws(() => readSheet(bomb, { maxUncompressedBytes: 1_000_000 }), { code: 'TOO_LARGE' });
});

test('sparse sheet with a far-away cell hits the cell limit instead of allocating', () => {
  // A row with a cell far to the right would be padded to a huge dense grid.
  const out = writeXlsx([{ name: 'S', columns: [], rows: [['a', ...new Array(16_000).fill(null), 'z']] }]);
  assert.throws(() => readSheet(out, { maxCells: 10_000 }), { code: 'TOO_LARGE' });
});

test('tiny csv that pads to a huge grid is rejected with default limits', () => {
  // 2,000 one-cell rows plus one row with 10,000 fields: 20 million cells after padding.
  const csv = Buffer.from('a\n'.repeat(2000) + ';'.repeat(9999) + '\n');
  const before = process.memoryUsage().rss;
  assert.throws(() => readSheet(csv, { csv: { delimiter: ';' } }), { code: 'TOO_LARGE' });
  assert.ok(process.memoryUsage().rss - before < 200 * 1024 * 1024);
});

test('a large read does not break the next call', () => {
  const rows = Array.from({ length: 200_000 }, (_, i) => [i, 'text', i * 2, 'more text', i]);
  readSheet(writeXlsx([{ name: 'S', columns: [], rows }]));
  // The next call still works on a fresh instance.
  assert.deepEqual(readSheet(fixture('utf8-bom.csv')).rows[0], ['a', 'b']);
});
