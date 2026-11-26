// Property tests for the schema: German notation in csv text round-trips.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import fc from 'fast-check';
import { lib } from './helpers.mjs';

const { readSheet, writeXlsx } = lib;
const RUNS = Number(process.env.EXCEL_RS_PROPERTY_RUNS ?? 300);

const csvCell = (s) => (/[;"\n\r]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s);
const csv = (rows) => Buffer.from(rows.map((r) => r.map(csvCell).join(';')).join('\r\n'), 'utf8');

const germanText = fc.string({
  unit: fc.constantFrom(...'aZ07 äöüÄÖÜß€§°;,."\n\t-\'é'.split('')),
  maxLength: 20,
});

test('numbers in German notation (1.234,56) are read exactly', () => {
  const de = new Intl.NumberFormat('de-DE', { maximumFractionDigits: 2, useGrouping: true });
  const plain = new Intl.NumberFormat('de-DE', { maximumFractionDigits: 2, useGrouping: false });
  fc.assert(
    fc.property(fc.integer({ min: -1e13, max: 1e13 }), fc.boolean(), (cents, grouped) => {
      const n = cents / 100;
      const text = (grouped ? de : plain).format(n).replace('−', '-');
      const res = readSheet(csv([['Betrag'], [text]]), { schema: { v: { header: 'Betrag', type: 'number' } } });
      assert.deepEqual(res.errors, [], text);
      assert.equal(res.rows[0].v, n, text);
    }),
    { numRuns: RUNS },
  );
});

test('German dates (31.12.2025 14:30) are read as UTC', () => {
  fc.assert(
    fc.property(
      fc.date({ min: new Date('1900-01-01T00:00:00Z'), max: new Date('2099-12-31T23:59:00Z'), noInvalidDate: true }),
      fc.boolean(),
      (d, withTime) => {
        const date = new Date(Math.floor(d.getTime() / 60_000) * 60_000);
        if (!withTime) date.setUTCHours(0, 0, 0, 0);
        const p = (x) => String(x).padStart(2, '0');
        let text = `${p(date.getUTCDate())}.${p(date.getUTCMonth() + 1)}.${date.getUTCFullYear()}`;
        if (withTime) text += ` ${p(date.getUTCHours())}:${p(date.getUTCMinutes())}`;
        const res = readSheet(csv([['Datum'], [text]]), { schema: { d: { header: 'Datum', type: 'date' } } });
        assert.deepEqual(res.errors, [], text);
        assert.equal(res.rows[0].d.getTime(), date.getTime(), text);
      },
    ),
    { numRuns: RUNS },
  );
});

test('text with umlauts, ; and quotes survives csv and xlsx, with row numbers', () => {
  fc.assert(
    fc.property(fc.array(fc.tuple(germanText, germanText), { minLength: 1, maxLength: 15 }), (pairs) => {
      const schema = { a: { header: 'Straße' }, b: { header: 'Ort' } };
      const want = [];
      const wantRows = [];
      pairs.forEach(([a, b], i) => {
        const [ta, tb] = [a.trim(), b.trim()];
        if (ta === '' && tb === '') return; // empty rows are skipped
        want.push({ a: ta === '' ? null : ta, b: tb === '' ? null : tb });
        wantRows.push(i + 2);
      });
      for (const bytes of [
        csv([['Straße', 'Ort'], ...pairs]),
        writeXlsx([{ name: 'S', columns: [{ header: 'Straße' }, { header: 'Ort' }], rows: pairs }]),
      ]) {
        const res = readSheet(bytes, { schema });
        assert.deepEqual(res.errors, []);
        assert.deepEqual(res.rows, want);
        assert.deepEqual(res.rowNumbers, wantRows);
      }
    }),
    { numRuns: RUNS },
  );
});
