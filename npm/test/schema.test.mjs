import { test } from 'node:test';
import assert from 'node:assert/strict';
import { lib } from './helpers.mjs';

const { readSheet, writeXlsx, ExcelError } = lib;

const csv = (text) => Buffer.from(text, 'utf8');
const xlsx = (header, rows) => writeXlsx([{ name: 'S', columns: header.map((h) => ({ header: h })), rows }]);

const schema = {
  name: { header: 'Name', required: true, max: 20 },
  qty: { header: 'Menge', aliases: ['Anzahl'], type: 'integer', required: true, min: 1 },
  price: { header: 'Preis', type: 'number' },
  due: { header: 'Fällig am', type: 'date' },
  active: { header: 'Aktiv', type: 'boolean' },
};

test('German csv: comma decimals, dates, ja/nein, header matched loosely', () => {
  const text = 'name;ANZAHL; Preis ;Fällig  am;Aktiv\nSchraube;1.200;1.234,5;31.12.2025;ja\nMutter;3;0,99;2026-01-02 14:30;nein\n';
  const res = readSheet(csv(text), { schema });
  assert.deepEqual(res.errors, []);
  assert.equal(res.rowCount, 2);
  assert.deepEqual(res.rows[0], {
    name: 'Schraube',
    qty: 1200,
    price: 1234.5,
    due: new Date(Date.UTC(2025, 11, 31)),
    active: true,
  });
  assert.deepEqual(res.rows[1].due, new Date(Date.UTC(2026, 0, 2, 14, 30)));
});

test('missing required column stops before the rows', () => {
  const res = readSheet(csv('Name;Preis\nA;1\n'), { schema });
  assert.deepEqual(res.rows, []);
  assert.deepEqual(res.errors, [
    { row: 1, column: 'Menge', code: 'MISSING_COLUMN', message: 'Die Spalte „Menge“ fehlt.' },
  ]);
});

test('row errors carry Excel row numbers, cell and German message', () => {
  const text = [
    'Name;Menge;Preis;Fällig am;Aktiv',
    'Gut;1;;;',
    ';2;;;', // empty required cell
    'X;abc;;;', // wrong type
    'Y;1,5;;;', // not an integer
    'Z;0;;;', // below min
    'W;1;1,2,3;31.02.2025;vielleicht', // three errors in one row
    ';;;;', // empty row: skipped, no error
    'Sehr langer Name der zu lang ist;1;;;',
  ].join('\n');
  const res = readSheet(csv(text), { schema });
  assert.deepEqual(res.rows, [{ name: 'Gut', qty: 1, price: null, due: null, active: null }]);
  assert.equal(res.rowCount, 7);
  assert.deepEqual(
    res.errors.map((e) => [e.row, e.cell, e.code]),
    [
      [3, 'A3', 'REQUIRED'],
      [4, 'B4', 'INVALID_TYPE'],
      [5, 'B5', 'INVALID_TYPE'],
      [6, 'B6', 'BELOW_MIN'],
      [7, 'C7', 'INVALID_TYPE'],
      [7, 'D7', 'INVALID_TYPE'],
      [7, 'E7', 'INVALID_TYPE'],
      [9, 'A9', 'ABOVE_MAX'],
    ],
  );
  const msg = Object.fromEntries(res.errors.map((e) => [e.cell, e.message]));
  assert.equal(msg.A3, '„Name“ darf nicht leer sein.');
  assert.equal(msg.B4, '„Menge“: „abc“ ist keine ganze Zahl.');
  assert.equal(msg.B6, '„Menge“: „0“ ist kleiner als 1.');
  assert.equal(msg.D7, '„Fällig am“: „31.02.2025“ ist kein gültiges Datum.');
  assert.equal(msg.A9, '„Name“: „Sehr langer Name der zu lang ist“ ist länger als 20 Zeichen.');
});

test('typed xlsx cells are taken as they are', () => {
  const due = new Date(Date.UTC(2025, 5, 1));
  const bytes = xlsx(['Name', 'Menge', 'Preis', 'Fällig am', 'Aktiv'], [['A', 2, 3.5, due, true]]);
  const res = readSheet(bytes, { schema });
  assert.deepEqual(res.errors, []);
  assert.deepEqual(res.rows, [{ name: 'A', qty: 2, price: 3.5, due, active: true }]);
});

test('pattern, allowed values, custom check, decimal point', () => {
  const s = {
    plz: { header: 'PLZ', pattern: /^\d{5}$/ },
    status: { header: 'Status', values: ['Offen', 'Erledigt'] },
    even: { header: 'Gerade', type: 'integer', validate: (n) => (n % 2 ? 'muss gerade sein.' : undefined) },
    amount: { header: 'Betrag', type: 'number', decimal: '.' },
  };
  const res = readSheet(csv('PLZ,Status,Gerade,Betrag\n12345,erledigt,2,"1,234.5"\n1234,Neu,3,1.5\n'), { schema: s });
  assert.deepEqual(res.rows, [{ plz: '12345', status: 'Erledigt', even: 2, amount: 1234.5 }]);
  assert.deepEqual(
    res.errors.map((e) => [e.code, e.message]),
    [
      ['PATTERN', '„PLZ“: „1234“ hat nicht das erwartete Format.'],
      ['NOT_ALLOWED', '„Status“: „Neu“ ist nicht erlaubt (erlaubt: „Offen“, „Erledigt“).'],
      ['CUSTOM', '„Gerade“: muss gerade sein.'],
    ],
  );
});

test('date bounds and German number notation errors', () => {
  const s = { d: { header: 'D', type: 'date', min: new Date(Date.UTC(2025, 0, 1)) }, n: { header: 'N', type: 'number' } };
  const res = readSheet(csv('D;N\n31.12.2024;1.23\n01.01.2025;12.345.678,9\n'), { schema: s });
  assert.deepEqual(res.rows, [{ d: new Date(Date.UTC(2025, 0, 1)), n: 12345678.9 }]);
  assert.deepEqual(
    res.errors.map((e) => e.message),
    ['„D“: „31.12.2024“ liegt vor dem 01.01.2025.', '„N“: „1.23“ ist keine Zahl.'],
  );
});

test('maxErrors caps the list', () => {
  const rows = Array.from({ length: 50 }, () => 'x').join('\n');
  const res = readSheet(csv(`N\n${rows}\n`), { schema: { n: { header: 'N', type: 'number' } }, maxErrors: 5 });
  assert.equal(res.errors.length, 5);
  assert.equal(res.errorsTruncated, true);
  assert.equal(res.rowCount, 50);
});

test('invalid schema is rejected before reading', () => {
  const bad = [
    null,
    {},
    { a: { header: '' } },
    { a: { header: 'A', type: 'money' } },
    { a: { header: 'A', pattern: '^x$' } },
    { a: { header: 'A', min: '1' } },
    JSON.parse('{"__proto__": {"header": "A"}}'),
  ];
  for (const schema of bad) {
    assert.throws(() => readSheet(csv('A\n1\n'), { schema }), (e) => e instanceof ExcelError && e.code === 'INVALID_INPUT');
  }
  assert.throws(() => readSheet(csv('A\n1\n'), { schema: { a: { header: 'A' } }, maxErrors: -1 }), { code: 'INVALID_INPUT' });
});

test('read options still apply with a schema', () => {
  assert.throws(() => readSheet(csv('A\n1\n2\n3\n'), { schema: { a: { header: 'A' } }, maxRows: 2 }), { code: 'TOO_LARGE' });
});
