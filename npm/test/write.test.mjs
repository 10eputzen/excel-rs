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

// --- sheet protection and styles -------------------------------------------

const files = (sheets) => XLSX.read(writeXlsx(sheets), { type: 'buffer', bookFiles: true }).files;
const part = (f, name) => f[name].content.toString();

/** Font, fill and alignment of a cell, read from the xlsx XML. */
function cellFormat(f, ref) {
  const sheet = part(f, 'xl/worksheets/sheet1.xml');
  const cell = new RegExp(`<c r="${ref}"([^>]*?)/?>`).exec(sheet);
  assert.ok(cell, `cell ${ref} exists`);
  const s = /\bs="(\d+)"/.exec(cell[1]);
  assert.ok(s, `cell ${ref} has a style`);
  const styles = part(f, 'xl/styles.xml');
  const section = (tag) => new RegExp(`<${tag}[ >][^]*?</${tag}>`).exec(styles)[0];
  const list = (tag, item) => {
    const re = item === 'xf' ? /<xf [^>]*?(?:\/>|>[^]*?<\/xf>)/g : new RegExp(`<${item}>[^]*?</${item}>`, 'g');
    return [...section(tag).matchAll(re)].map((m) => m[0]);
  };
  const xf = list('cellXfs', 'xf')[Number(s[1])];
  const font = list('fonts', 'font')[Number(/fontId="(\d+)"/.exec(xf)[1])];
  const fill = list('fills', 'fill')[Number(/fillId="(\d+)"/.exec(xf)[1])];
  return {
    bold: /<b\/>/.test(font),
    size: Number(/<sz val="([\d.]+)"/.exec(font)?.[1]),
    color: /<color rgb="([0-9A-F]{8})"/.exec(font)?.[1],
    fill: /<fgColor rgb="([0-9A-F]{8})"/.exec(fill)?.[1],
    align: /<alignment [^>]*horizontal="(\w+)"/.exec(xf)?.[1],
  };
}

const PLAIN = { name: 'S', columns: [{ header: 'A' }, { header: 'B' }], rows: [['x', 1], ['y', 2]] };

test('protect locks the sheet without a password', () => {
  const on = part(files([{ ...PLAIN, protect: true }]), 'xl/worksheets/sheet1.xml');
  assert.match(on, /<sheetProtection [^>]*sheet="1"/);
  assert.doesNotMatch(on, /<sheetProtection [^>]*password=/);
  assert.doesNotMatch(part(files([PLAIN]), 'xl/worksheets/sheet1.xml'), /<sheetProtection/);
  assert.doesNotMatch(part(files([{ ...PLAIN, protect: false }]), 'xl/worksheets/sheet1.xml'), /<sheetProtection/);
});

test('headerStyle replaces the default header style', () => {
  const f = files([{ ...PLAIN, headerStyle: { bold: true, fontColor: '#FFFFFF', fill: '1F4E78', align: 'center', fontSize: 12 } }]);
  assert.deepEqual(cellFormat(f, 'A1'), { bold: true, size: 12, color: 'FFFFFFFF', fill: 'FF1F4E78', align: 'center' });
  assert.equal(cellFormat(f, 'B1').fill, 'FF1F4E78');
  const off = files([{ ...PLAIN, headerStyle: { fill: '#FFFF00' } }]);
  assert.equal(cellFormat(off, 'A1').bold, false);
  assert.equal(cellFormat(off, 'A1').fill, 'FFFFFF00');
});

test('titleRows come before the header; mergeAcross merges from A', () => {
  const sheet = {
    ...PLAIN,
    columns: [{ header: 'A' }, { header: 'B' }, { header: 'C' }, { header: 'D' }],
    rows: [['x', 1, 2, 3]],
    titleRows: [{ value: 'Übersicht', style: { bold: true, fontSize: 14 }, mergeAcross: 4 }, { value: null }],
  };
  const f = files([sheet]);
  const xml = part(f, 'xl/worksheets/sheet1.xml');
  assert.match(xml, /<mergeCell ref="A1:D1"\/>/);
  assert.match(xml, /<pane [^>]*ySplit="3"/);
  assert.match(xml, /<autoFilter ref="A3:D4"/);
  assert.deepEqual(cellFormat(f, 'A1').bold, true);
  assert.equal(cellFormat(f, 'A1').size, 14);
  const res = readSheet(writeXlsx([sheet]));
  assert.deepEqual(res.rows, [['Übersicht', null, null, null], [null, null, null, null], ['A', 'B', 'C', 'D'], ['x', 1, 2, 3]]);
});

test('titleRows accept numbers and dates; a title does not widen column A', () => {
  const sheet = {
    ...PLAIN,
    titleRows: [{ value: day(2026, 11, 27) }, { value: 42 }, { value: 'T'.repeat(70), mergeAcross: 2 }],
  };
  const res = readSheet(writeXlsx([sheet]));
  assert.equal(res.rows[0][0].toISOString(), '2026-11-27T00:00:00.000Z');
  assert.equal(res.rows[1][0], 42);
  const xml = part(files([sheet]), 'xl/worksheets/sheet1.xml');
  const width = Number(/<col [^>]*max="1"[^>]*width="([\d.]+)"/.exec(xml)?.[1] ?? /<col [^>]*width="([\d.]+)"[^>]*max="1"/.exec(xml)?.[1]);
  assert.ok(width < 20, `column A width ${width}`);
});

test('rowStyles and cellStyles; cell wins over row, empty cells are colored', () => {
  const f = files([
    {
      name: 'S',
      columns: [{ header: 'A' }, { header: 'B', format: 'date' }, { header: 'C' }],
      rows: [['a', day(2025, 3, 1), 'x'], ['b', null], ['c', 3, 4]],
      rowStyles: [{ fill: '#FFC7CE', fontColor: '#9C0006' }, { bold: true, fill: '#C6EFCE' }, null],
      cellStyles: [[null, { fill: '#FFFF00' }], null, [undefined, undefined, { bold: true }]],
    },
  ]);
  assert.deepEqual(cellFormat(f, 'A2'), { bold: false, size: 11, color: 'FF9C0006', fill: 'FFFFC7CE', align: undefined });
  assert.equal(cellFormat(f, 'B2').fill, 'FFFFFF00', 'cell fill wins');
  assert.equal(cellFormat(f, 'B2').color, 'FF9C0006', 'row font color stays');
  assert.equal(cellFormat(f, 'A3').bold, true);
  assert.equal(cellFormat(f, 'B3').fill, 'FFC6EFCE', 'empty cell is colored');
  assert.equal(cellFormat(f, 'C3').fill, 'FFC6EFCE', 'cells the row does not fill are colored');
  assert.equal(cellFormat(f, 'C4').bold, true);
  // Styles do not change values, and a style keeps the column's date format.
  const res = readSheet(writeXlsx([{ name: 'S', columns: [{ header: 'B', format: 'date' }], rows: [[day(2025, 3, 1)]], rowStyles: [{ bold: true }] }]));
  assert.equal(res.rows[1][0].toISOString(), '2025-03-01T00:00:00.000Z');
});

test('styled cells keep numbers, booleans and text intact', () => {
  const rows = [['t', 1.5, true], ['u', -2, false]];
  const res = readSheet(writeXlsx([{ name: 'S', columns: [], rows, rowStyles: [{ bold: true, fill: '#EEEEEE' }, { align: 'right' }] }]));
  assert.deepEqual(res.rows, rows);
});

test('invalid styles are rejected with INVALID_INPUT', () => {
  const bad = [
    { headerStyle: { fill: 'red' } },
    { headerStyle: { fill: '#12345' } },
    { headerStyle: { fontColor: 5 } },
    { headerStyle: { align: 'middle' } },
    { headerStyle: { bold: 'yes' } },
    { headerStyle: { fontSize: 0 } },
    { headerStyle: 'bold' },
    { rowStyles: 'x' },
    { rowStyles: [{ fill: 'nope' }] },
    { cellStyles: [[{ align: 1 }]] },
    { cellStyles: 'x' },
    { titleRows: 'x' },
    { titleRows: [{ value: 'a', mergeAcross: 0 }] },
    { titleRows: [{ value: 'a', mergeAcross: 1.5 }] },
    { titleRows: [{ value: 'a', mergeAcross: 99999 }] },
    { titleRows: [{ value: { x: 1 } }] },
    { protect: 'yes' },
  ];
  for (const extra of bad) {
    assert.throws(() => writeXlsx([{ ...PLAIN, ...extra }]), { code: 'INVALID_INPUT' }, JSON.stringify(extra));
  }
});
