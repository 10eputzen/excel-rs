import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { lib, plain } from './helpers.mjs';

const { readSheet, writeXlsx } = lib;
const XLSX = createRequire(import.meta.url)('xlsx');

const day = (y, m, d) => new Date(Date.UTC(y, m - 1, d));

const SHEET = {
  name: 'Bestellungen',
  columns: [
    { header: 'Kunde' },
    { header: 'Datum', format: 'date' },
    { header: 'Zeitpunkt', format: 'datetime' },
    { header: 'Betrag', format: '#,##0.00', width: 14 },
    { header: 'PLZ', format: 'text' },
    { header: 'Bezahlt' },
  ],
  rows: [
    ['Müller & Söhne', day(2024, 2, 29), new Date(Date.UTC(2024, 2, 1, 8, 30, 15)), 1234.5, '01067', true],
    ['=HYPERLINK("x")', day(1900, 1, 1), null, -0.01, '', false],
    [null, day(9999, 12, 31), new Date(Date.UTC(1970, 0, 1, 23, 59, 59, 500)), 0, '80331', null],
  ],
};

test('round trip keeps values and types', () => {
  const out = writeXlsx([SHEET]);
  assert.ok(out instanceof Uint8Array);
  const res = readSheet(out);
  assert.deepEqual(res.sheetNames, ['Bestellungen']);
  assert.deepEqual(res.rows[0], SHEET.columns.map((c) => c.header));
  // Empty strings are written as empty cells.
  const expected = SHEET.rows.map((r) => r.map((v) => (v === '' ? null : v)));
  assert.deepEqual(plain(res.rows.slice(1)), plain(expected));
});

test('formula-like strings stay text', () => {
  const res = readSheet(writeXlsx([SHEET]));
  assert.equal(res.rows[2][0], '=HYPERLINK("x")');
  const wb = XLSX.read(writeXlsx([SHEET]), { type: 'buffer' });
  assert.equal(wb.Sheets.Bestellungen.A3.f, undefined);
});

test('SheetJS reads the export identically (third-party reader check)', () => {
  const out = writeXlsx([SHEET, { name: 'Zwei', columns: [], rows: [[1, 'a']] }]);
  const wb = XLSX.read(out, { type: 'buffer', cellDates: true, cellNF: true, UTC: true });
  assert.deepEqual(wb.SheetNames, ['Bestellungen', 'Zwei']);
  const ws = wb.Sheets.Bestellungen;
  assert.equal(ws.A2.v, 'Müller & Söhne');
  assert.equal(ws.B2.t, 'd');
  assert.equal(ws.B2.v.toISOString(), '2024-02-29T00:00:00.000Z');
  assert.equal(ws.C2.v.toISOString(), '2024-03-01T08:30:15.000Z');
  assert.equal(ws.D2.v, 1234.5);
  assert.equal(ws.D2.z, '#,##0.00');
  assert.equal(ws.E2.t, 's');
  assert.equal(ws.E2.v, '01067');
  assert.equal(ws.F2.v, true);
  assert.equal(ws['!autofilter']?.ref, 'A1:F4');
  assert.deepEqual(XLSX.utils.sheet_to_json(wb.Sheets.Zwei, { header: 1 }), [[1, 'a']]);
});

test('header is bold, frozen and filtered; options turn it off', () => {
  const read = (opts) => XLSX.read(writeXlsx([{ ...SHEET, ...opts }]), { type: 'buffer', cellStyles: true });
  const on = read({});
  assert.equal(on.Sheets.Bestellungen['!autofilter']?.ref, 'A1:F4');
  const off = read({ autoFilter: false, freezeHeader: false });
  assert.equal(off.Sheets.Bestellungen['!autofilter'], undefined);
  const part = (opts, name) =>
    XLSX.read(writeXlsx([{ ...SHEET, ...opts }]), { type: 'buffer', bookFiles: true }).files[name].content.toString();
  assert.match(part({}, 'xl/worksheets/sheet1.xml'), /<pane [^>]*ySplit="1"[^>]*state="frozen"/);
  assert.doesNotMatch(part({ freezeHeader: false }, 'xl/worksheets/sheet1.xml'), /<pane /);
  const styles = part({}, 'xl/styles.xml');
  assert.match(styles, /<b\/>/);
});

test('without columns no header row is written', () => {
  const res = readSheet(writeXlsx([{ name: 'S', columns: [], rows: [[1, 2], ['a']] }]));
  assert.deepEqual(res.rows, [
    [1, 2],
    ['a', null],
  ]);
});

test('undefined, NaN and invalid dates become empty cells', () => {
  const res = readSheet(
    writeXlsx([{ name: 'S', columns: [], rows: [['x', undefined, NaN, new Date('nope'), Infinity, 1n]] }]),
  );
  assert.deepEqual(res.rows, [['x', null, null, null, null, 1]]);
});

test('invalid input is rejected with INVALID_INPUT', () => {
  const bad = [
    [],
    [{ name: '', columns: [], rows: [] }],
    [{ name: 'a/b', columns: [], rows: [] }],
    [{ name: 'S', columns: [], rows: [[{ x: 1 }]] }],
    [{ name: 'S', columns: [], rows: [[[1]]] }],
    [{ name: 'S', columns: [{ header: 1 }], rows: [] }],
    [{ name: 'S', columns: [{ header: 'a', width: -1 }], rows: [] }],
    [{ name: 'S', columns: [], rows: 'x' }],
    [
      { name: 'S', columns: [], rows: [] },
      { name: 'S', columns: [], rows: [] },
    ],
    [{ name: 'S', columns: [], rows: [[day(1800, 1, 1)]] }],
    [{ name: 'S', columns: [], rows: [['x'.repeat(40000)]] }],
  ];
  for (const sheets of bad) {
    assert.throws(() => writeXlsx(sheets), { code: 'INVALID_INPUT' }, JSON.stringify(sheets).slice(0, 80));
  }
  assert.throws(() => writeXlsx('nope'), { code: 'INVALID_INPUT' });
});

test('random data survives a round trip (property test)', () => {
  let seed = 42;
  const rand = () => ((seed = (seed * 1103515245 + 12345) % 2 ** 31) / 2 ** 31);
  const chars = 'aäöüßAZ09 €"\';,\n\t<>&';
  const cell = () => {
    const r = rand();
    if (r < 0.15) return null;
    if (r < 0.35) return Math.round((rand() - 0.5) * 1e9) / 1000;
    if (r < 0.45) return rand() < 0.5;
    if (r < 0.6) return new Date(Date.UTC(1901 + Math.floor(rand() * 200), Math.floor(rand() * 12), 1 + Math.floor(rand() * 28), Math.floor(rand() * 24), Math.floor(rand() * 60), Math.floor(rand() * 60)));
    const len = 1 + Math.floor(rand() * 20);
    let s = '';
    for (let i = 0; i < len; i++) s += chars[Math.floor(rand() * chars.length)];
    return s;
  };
  for (let round = 0; round < 20; round++) {
    const width = 1 + Math.floor(rand() * 8);
    const rows = Array.from({ length: 1 + Math.floor(rand() * 50) }, () => Array.from({ length: width }, cell));
    rows[0][width - 1] = 'end'; // keep the used range rectangular
    rows[rows.length - 1][width - 1] = 'end';
    const res = readSheet(writeXlsx([{ name: 'R', columns: [], rows }]));
    assert.deepEqual(plain(res.rows), plain(rows), `round ${round}`);
  }
});
