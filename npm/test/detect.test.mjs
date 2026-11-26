import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { gzipSync } from 'node:zlib';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { fixture, fixturesDir, lib } from './helpers.mjs';

const detect = await import('../dist/browser/detect.js');
const { detectFormat, readCsvHeader, missingColumns } = detect;
const head = (bytes) => new Uint8Array(bytes.subarray(0, 4096));

test('detects the format from the first bytes of real files', () => {
  const expected = { xlsx: 'xlsx', xlsm: 'xlsx', xlsb: 'xlsx', ods: 'ods', xls: 'xls', csv: 'csv' };
  let checked = 0;
  for (const file of readdirSync(fixturesDir)) {
    const ext = file.split('.').pop();
    if (!(ext in expected)) continue;
    assert.equal(detectFormat(head(fixture(file))), expected[ext], file);
    checked++;
  }
  assert.ok(checked >= 5);
  const written = lib.writeXlsx([{ name: 'S', columns: [], rows: [[1]] }]);
  assert.equal(detectFormat(head(written)), 'xlsx');
});

test('text and binary input', () => {
  const enc = (s) => new TextEncoder().encode(s);
  assert.equal(detectFormat(enc('a;b\n1;2\n')), 'csv');
  assert.equal(detectFormat(new Uint8Array([0xef, 0xbb, 0xbf, 0x61])), 'csv');
  assert.equal(detectFormat(new Uint8Array([0xff, 0xfe, 0x61, 0x00])), 'csv');
  assert.equal(detectFormat(Buffer.from('Müller;Straße\n', 'latin1')), 'csv');
  assert.equal(detectFormat(enc('%PDF-1.7\n')), 'unknown');
  assert.equal(detectFormat(new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0, 1, 2])), 'unknown');
  assert.equal(detectFormat(new Uint8Array()), 'unknown');
});

test('csv header: delimiter, quotes, BOM, line breaks inside quotes', () => {
  assert.deepEqual(readCsvHeader('﻿Name;Menge;"Preis; brutto"\r\n1;2;3'), {
    delimiter: ';',
    columns: ['Name', 'Menge', 'Preis; brutto'],
  });
  assert.deepEqual(readCsvHeader('a,"b ""x""\nc",d\n1,2,3'), { delimiter: ',', columns: ['a', 'b "x"\nc', 'd'] });
  assert.deepEqual(readCsvHeader('a\tb\n'), { delimiter: '\t', columns: ['a', 'b'] });
  assert.deepEqual(readCsvHeader(''), { delimiter: ';', columns: [''] });
});

test('missing required columns, matched like readSheet', () => {
  const schema = {
    a: { header: 'Name', required: true },
    b: { header: 'Menge', aliases: ['Anzahl'], required: true },
    c: { header: 'Preis', required: true },
    d: { header: 'Notiz' },
  };
  assert.deepEqual(missingColumns([' name ', 'ANZAHL'], schema), ['Preis']);
});

test('detect entry stays small and works without WebAssembly', () => {
  const file = fileURLToPath(new URL('../dist/browser/detect.js', import.meta.url));
  const source = readFileSync(file, 'utf8');
  assert.ok(gzipSync(source).length < 2048, `${gzipSync(source).length} bytes gzipped`);
  assert.doesNotMatch(source, /\bimport\b|\.wasm/);
  const cjs = createRequire(import.meta.url)('../dist/node/detect.js');
  assert.equal(cjs.detectFormat(new TextEncoder().encode('a;b')), 'csv');
});
