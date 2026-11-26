// The WebAssembly build reads the files saved by Microsoft Excel exactly like
// the native Rust build: compares against the golden results in fixtures/excel.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { lib, fixture, fixturesDir } from './helpers.mjs';

const { readSheet } = lib;
const dir = `${fixturesDir}/excel`;
const files = readdirSync(dir).filter((f) => /\.(xlsx|xls|xlsb)$/.test(f));

/** Golden JSON → values as the Node API returns them. */
const revive = (rows) => rows.map((r) => r.map((v) => (v !== null && typeof v === 'object' ? new Date(v.d) : v)));

test('files saved by Excel match the golden results', () => {
  assert.ok(files.length >= 10);
  for (const file of files) {
    const golden = JSON.parse(readFileSync(`${dir}/${file}.json`, 'utf8'));
    const bytes = fixture(`excel/${file}`);
    golden.sheetNames.forEach((name, i) => {
      const want = golden.sheets[name];
      if (want.error) {
        assert.throws(() => readSheet(bytes, { sheet: i }), { code: want.error.replace(/([a-z])([A-Z])/g, '$1_$2').toUpperCase() });
        return;
      }
      const res = readSheet(bytes, { sheet: i });
      assert.deepEqual(res.sheetNames, golden.sheetNames, file);
      assert.deepEqual(res.rows, revive(want), `${file} / ${name}`);
    });
  }
});
