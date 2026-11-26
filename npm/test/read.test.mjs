import { test } from 'node:test';
import assert from 'node:assert/strict';
import { lib, fixture, plain } from './helpers.mjs';

const { readSheet, ExcelError } = lib;

const REFERENCE = [
  ['Text', 'Zahl', 'Datum', 'Wahr', 'Formel', 'Leer'],
  ['Größe Äpfel ß €', 1234.56, '2024-02-29T00:00:00.000Z', true, 2469.12, null],
  [null, -0.5, '2023-12-31T13:45:00.000Z', false, -1, null],
  ['Zusammengeführt', null, null, null, null, 'Ende'],
];

for (const file of [
  'reference-sheetjs.xlsx',
  'reference-libreoffice.xlsx',
  'reference-libreoffice.xls',
  'reference-libreoffice.ods',
]) {
  test(`reads ${file}`, () => {
    const res = readSheet(fixture(file));
    assert.deepEqual(res.sheetNames, ['Daten', 'Zweite']);
    assert.equal(res.sheet, 'Daten');
    assert.deepEqual(plain(res.rows), REFERENCE);
    assert.ok(res.rows[1][2] instanceof Date);
  });

  test(`selects sheets in ${file}`, () => {
    assert.deepEqual(readSheet(fixture(file), { sheet: 'Zweite' }).rows, [['x'], [1]]);
    assert.deepEqual(readSheet(fixture(file), { sheet: 1 }).rows, [['x'], [1]]);
    assert.throws(() => readSheet(fixture(file), { sheet: 'Fehlt' }), { code: 'SHEET_NOT_FOUND' });
    assert.throws(() => readSheet(fixture(file), { sheet: 7 }), { code: 'SHEET_NOT_FOUND' });
  });
}

// SheetJS writes xlsb "short" cell records (no column index), which the parser
// does not support yet. Files written by Excel need to be verified separately.
test('reads xlsb written by SheetJS', { todo: 'short cell records (BrtShort*) are not supported yet' }, () => {
  assert.deepEqual(plain(readSheet(fixture('reference-sheetjs.xlsb')).rows), REFERENCE);
});

test('detects xlsb and reads its sheet names', () => {
  assert.deepEqual(readSheet(fixture('reference-sheetjs.xlsb')).sheetNames, ['Daten', 'Zweite']);
});

test('reads German Excel csv (semicolon, Windows-1252)', () => {
  const res = readSheet(fixture('german-excel.csv'));
  assert.deepEqual(res.sheetNames, ['Sheet1']);
  assert.deepEqual(res.rows, [
    ['Name', 'Betrag', 'Datum'],
    ['Müller', '1.234,56', '29.02.2024'],
    ['Schmidt; Söhne', '-0,5', null],
  ]);
});

test('reads UTF-8 csv with BOM and quoted newline', () => {
  assert.deepEqual(readSheet(fixture('utf8-bom.csv')).rows, [
    ['a', 'b'],
    ['x\ny', '€'],
  ]);
});

test('csv options override detection', () => {
  const bytes = Buffer.from('a;b,c\n1;2,3\n');
  assert.deepEqual(readSheet(bytes, { csv: { delimiter: ',' } }).rows, [
    ['a;b', 'c'],
    ['1;2', '3'],
  ]);
  const latin = Buffer.from('Gr\xfc\xdfe\n', 'latin1');
  assert.deepEqual(readSheet(latin, { format: 'csv', csv: { encoding: 'windows-1252' } }).rows, [['Grüße']]);
  assert.throws(() => readSheet(latin, { csv: { encoding: 'utf-8' } }), { code: 'CORRUPT_FILE' });
});

test('accepts ArrayBuffer and Uint8Array', () => {
  const buf = fixture('utf8-bom.csv');
  const ab = buf.buffer.slice(buf.byteOffset, buf.byteOffset + buf.byteLength);
  assert.deepEqual(readSheet(ab).rows, readSheet(new Uint8Array(buf)).rows);
  assert.throws(() => readSheet('nope'), { code: 'INVALID_INPUT' });
});

test('invalid options are rejected instead of ignored', () => {
  const csv = fixture('utf8-bom.csv');
  for (const opts of [{ sheet: NaN }, { sheet: -1 }, { sheet: 1.5 }, { maxRows: NaN }, { maxBytes: -1 }, { maxCells: '5' }, null]) {
    assert.throws(() => readSheet(csv, opts), { code: 'INVALID_INPUT' }, String(JSON.stringify(opts)));
  }
});

test('empty csv gives no rows', () => {
  assert.deepEqual(readSheet(Buffer.from('')).rows, []);
});

test('errors are ExcelError instances', () => {
  try {
    readSheet(Buffer.from([0, 1, 2, 3]));
    assert.fail('expected an error');
  } catch (err) {
    assert.ok(err instanceof ExcelError);
    assert.ok(err instanceof Error);
    assert.equal(err.name, 'ExcelError');
    assert.equal(err.code, 'UNSUPPORTED_FORMAT');
  }
});

test('explicit format mismatch is reported, not crashed', () => {
  assert.throws(() => readSheet(fixture('utf8-bom.csv'), { format: 'xlsx' }), { code: 'CORRUPT_FILE' });
  assert.throws(() => readSheet(fixture('reference-libreoffice.xlsx'), { format: 'xls' }), {
    code: 'CORRUPT_FILE',
  });
  assert.throws(() => readSheet(Buffer.from('a'), { format: 'pdf' }), { code: 'UNSUPPORTED_FORMAT' });
});
