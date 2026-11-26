// Regenerates the reference files in ../fixtures. Needs LibreOffice (soffice) on PATH.
// Base workbook is written with SheetJS, then converted by LibreOffice so that the
// fixtures come from two independent writers.
import { execFileSync } from 'node:child_process';
import { mkdtempSync, writeFileSync, copyFileSync, readdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import * as fs from 'node:fs';
import * as XLSX from 'xlsx';

XLSX.set_fs(fs);

const fixtures = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../fixtures');
const tmp = mkdtempSync(path.join(tmpdir(), 'excel-rs-fixtures-'));

const serial = (iso) => Date.parse(iso) / 86400000 + 25569;
const ws = {
  '!ref': 'A1:F4',
  A1: { t: 's', v: 'Text' }, B1: { t: 's', v: 'Zahl' }, C1: { t: 's', v: 'Datum' },
  D1: { t: 's', v: 'Wahr' }, E1: { t: 's', v: 'Formel' }, F1: { t: 's', v: 'Leer' },
  A2: { t: 's', v: 'Größe Äpfel ß €' }, B2: { t: 'n', v: 1234.56 },
  C2: { t: 'n', v: serial('2024-02-29T00:00:00Z'), z: 'yyyy-mm-dd' },
  D2: { t: 'b', v: true }, E2: { t: 'n', v: 2469.12, f: 'B2*2' },
  B3: { t: 'n', v: -0.5 },
  C3: { t: 'n', v: serial('2023-12-31T13:45:00Z'), z: 'yyyy-mm-dd hh:mm' },
  D3: { t: 'b', v: false }, E3: { t: 'n', v: -1, f: 'B3*2' },
  A4: { t: 's', v: 'Zusammengeführt' }, F4: { t: 's', v: 'Ende' },
  '!merges': [{ s: { r: 3, c: 0 }, e: { r: 3, c: 1 } }],
};
const ws2 = XLSX.utils.aoa_to_sheet([['x'], [1]]);
const wb = XLSX.utils.book_new();
XLSX.utils.book_append_sheet(wb, ws, 'Daten');
XLSX.utils.book_append_sheet(wb, ws2, 'Zweite');

XLSX.writeFile(wb, path.join(fixtures, 'reference-sheetjs.xlsx'));
XLSX.writeFile(wb, path.join(fixtures, 'reference-sheetjs.xlsb'));

// LibreOffice: convert the SheetJS file to xlsx, xls and ods.
const src = path.join(tmp, 'reference.xlsx');
XLSX.writeFile(wb, src);
for (const [filter, ext] of [['xlsx:Calc MS Excel 2007 XML', 'xlsx'], ['xls:MS Excel 97', 'xls'], ['ods', 'ods']]) {
  const out = path.join(tmp, ext);
  execFileSync('soffice', ['--headless', '--convert-to', filter, '--outdir', out, src], { stdio: 'ignore' });
  const [file] = readdirSync(out);
  copyFileSync(path.join(out, file), path.join(fixtures, `reference-libreoffice.${ext}`));
}

// German Excel CSV: semicolon, Windows-1252, CRLF, decimal comma.
const csv = 'Name;Betrag;Datum\r\nMüller;1.234,56;29.02.2024\r\n"Schmidt; Söhne";-0,5;\r\n';
writeFileSync(path.join(fixtures, 'german-excel.csv'), Buffer.from(csv, 'latin1'));
// UTF-8 with BOM, comma, quoted newline.
writeFileSync(path.join(fixtures, 'utf8-bom.csv'), '﻿a,b\n"x\ny",€\n');
console.log('fixtures written to', fixtures);
