# excel-rs

[![npm](https://img.shields.io/npm/v/@10eputzen/excel-rs)](https://www.npmjs.com/package/@10eputzen/excel-rs) [![CI](https://github.com/10eputzen/excel-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/10eputzen/excel-rs/actions/workflows/ci.yml) [![license](https://img.shields.io/npm/l/@10eputzen/excel-rs)](LICENSE)

Fast, strict spreadsheet import and export for Node.js. Reads xlsx, xlsm, xls, xlsb, ods and csv, validates rows against a schema with messages for end users, and writes xlsx. A Rust core compiled to WebAssembly: one package that runs wherever Node.js runs (tested on Linux and Windows), no native binaries, no postinstall, no runtime dependencies.

```sh
npm install @10eputzen/excel-rs
```

```ts
import { readSheet } from '@10eputzen/excel-rs';

const { rows, errors } = readSheet(bytes, {
  schema: {
    name: { header: 'Name', required: true },
    qty: { header: 'Menge', type: 'integer', min: 1 },
    due: { header: 'Fällig am', type: 'date' },
  },
});
// rows:   [{ name: 'Schraube', qty: 1200, due: 2025-12-31T00:00:00.000Z }, …]
// errors: [{ row: 7, cell: 'B7', code: 'INVALID_TYPE', message: '„Menge“: „abc“ ist keine ganze Zahl.' }]
```

## Why excel-rs

- **Fast.** Reads xlsx 4–6× faster than SheetJS and exceljs, csv more than 10× faster, and writes xlsx faster too, see [Benchmarks](#benchmarks).
- **Safe for uploads from strangers.** Limits for size, rows and cells are checked before memory is allocated; zip bombs, cyclic OLE files and parser crashes end in a typed `ExcelError`, never in a crashed process. Fuzzed weekly.
- **Exact row numbers.** Errors and valid rows carry the row number Excel shows, also for csv files with blank lines.
- **Small surface.** Two functions, `readSheet` and `writeXlsx`, plus a 2 KB browser check before the upload.

| Entry | Use | API |
|---|---|---|
| `@10eputzen/excel-rs` | Node.js 20+ (CommonJS and ESM), also serverless | synchronous |
| `@10eputzen/excel-rs/detect` | browsers: check a file before uploading it | pure TypeScript, under 2 KB gzip |
| `@10eputzen/excel-rs/browser` | deprecated, see [Browser](#browser-deprecated) | `async`, loads 0.8 MB of WebAssembly |

The intended setup for web apps: the browser checks the file with `@10eputzen/excel-rs/detect` and uploads it; the server reads it with `readSheet` and a schema, shows the user a preview with the errors, and writes only after confirmation.

## Benchmarks

<!-- bench:start -->
| Library | Read xlsx | Read csv (;) | Write xlsx | Peak memory (xlsx read) |
| --- | ---: | ---: | ---: | ---: |
| **excel-rs** | 570 ms | 210 ms | 1,559 ms | 528 MB |
| SheetJS 0.20.3 | 3,656 ms (6.4×) | 2,691 ms (12.8×) | 2,667 ms (1.7×) | 787 MB |
| exceljs 4.4.0 | 2,513 ms (4.4×) | 40,853 ms (194.2×) | 5,259 ms (3.4×) | 1597 MB |
| read-excel-file 9.3.14 | 1,239 ms (2.2×) | – | – | 614 MB |

100,000 rows × 10 columns (text with umlauts, numbers, booleans, dates). Median of 5 runs after a warm-up, each library in its own process; (n×) = slower than excel-rs by that factor. Node 22.23.5, Intel(R) Xeon(R) Processor @ 2.30GHz, 2026-11-26.
<!-- bench:end -->

Details and how to run it: [docs/benchmarks.md](docs/benchmarks.md). The CI fails if excel-rs reads xlsx less than 2× faster than SheetJS, or if the WebAssembly binary grows beyond 900 KB gzip.

## How it is tested

- **Files saved by Microsoft Excel** (xlsx, xls, xlsb; Windows, Mac and Online) with the expected result of every sheet as golden file, cross-checked against SheetJS ([fixtures/excel](fixtures/excel/SOURCES.md)). Plus reference files from LibreOffice and SheetJS with German content.
- **Property tests** with thousands of random sheets: umlauts, `ß`, `€`, `;` and quotes inside cells, line breaks, blank rows, LF and CRLF, Windows-1252, German numbers and dates must survive a round trip.
- **Fuzzing** of the reader and writer with cargo-fuzz, weekly in CI. Every crash found becomes a regression test ([fixtures/broken](fixtures/broken/README.md)).
- **Malicious files**: zip bombs, far-away cells, huge record counts, truncated and corrupted files.
- **CI on Linux and Windows**, Node.js 20, 22 and 24, plus a browser test in Chromium.

## Reading

```ts
import { readFileSync } from 'node:fs';
import { readSheet } from '@10eputzen/excel-rs';

const { sheetNames, sheet, rows } = readSheet(readFileSync('orders.xlsx'));
// rows: (string | number | boolean | Date | null)[][]
```

- The format is detected from the file content (`format: 'auto'`); you can force one with `format`.
- `rows` starts at cell A1. Every row has the same length; empty cells are `null`.
- Formula cells return their last calculated value.
- Date cells become `Date` objects in UTC (Excel stores no time zone: a cell showing `2024-02-29` becomes `2024-02-29T00:00:00.000Z`). Durations (e.g. `[h]:mm`) are returned as a number of days.
- Merged cells: the value is in the top-left cell; the other cells are `null`.
- Error cells (`#DIV/0!` …) are `null`.

### Options

```ts
readSheet(bytes, {
  format: 'auto',          // 'xlsx' | 'xls' | 'xlsb' | 'ods' | 'csv'
  sheet: 'Orders',         // sheet name or zero-based index, default: first sheet
  csv: {
    delimiter: 'auto',     // ',' | ';' | '\t' | '|'
    encoding: 'auto',      // 'utf-8' | 'windows-1252'
  },
  maxBytes: 50 * 1024 ** 2,              // input size
  maxUncompressedBytes: 500 * 1024 ** 2, // zip-based formats, checked before parsing
  maxRows: 1_048_576,
  maxCells: 5_000_000,                   // rows × columns of the result
});
```

Invalid option values throw `INVALID_INPUT`. The row and cell limits are enforced before the result is allocated: while reading for xlsx and xlsb, and by a scan of the raw records for xls and ods (whose parser loads all sheets at once, so there the limit applies to all sheets together). As a last line of defence the WebAssembly memory is capped at 2 GiB. After a call that grew the WebAssembly memory beyond 256 MiB, the module is reloaded so that the memory is released.

### CSV

CSV files exported by spreadsheet programs in many European locales use `;` as delimiter and Windows-1252 as encoding. With the default options both are detected:

- UTF-8 and UTF-16 byte order marks are removed; text that is not valid UTF-8 is decoded as Windows-1252. A few rare letter pairs are valid in both (`Ä€` is also UTF-8 for `Ā`); pass `encoding: 'windows-1252'` when you know the source.
- The delimiter is the candidate (`;`, `,`, tab, `|`) that appears most consistently in the first lines, ignoring quoted text. It must occur in the first line: a file whose header has none is read as a single column, so `-0,01` stays one cell.
- Blank lines are kept as empty rows, like Excel shows them, so row numbers match.

CSV cells are always strings (or `null` when empty). Number and date formats in CSV depend on the locale of the exporting program, so the conversion is left to you.

## Validating with a schema

```ts
import { readSheet } from '@10eputzen/excel-rs';

const { rows, rowNumbers, errors, rowCount } = readSheet(bytes, {
  schema: {
    name: { header: 'Name', required: true, max: 80 },
    qty: { header: 'Menge', aliases: ['Anzahl', 'Stück'], type: 'integer', required: true, min: 1 },
    price: { header: 'Preis', type: 'number' },
    due: { header: 'Fällig am', type: 'date', min: new Date('2020-01-01') },
    zip: { header: 'PLZ', pattern: /^\d{5}$/ },
    status: { header: 'Status', values: ['Offen', 'Erledigt'] },
    email: { header: 'E-Mail', validate: (v) => (v.includes('@') ? undefined : 'ist keine E-Mail-Adresse.') },
  },
  maxRows: 10_000,
});
// rows:   { name: string; qty: number; price: number | null; due: Date | null; … }[]
// errors: [{ row: 7, column: 'Menge', cell: 'B7', code: 'INVALID_TYPE',
//            message: '„Menge“: „abc“ ist keine ganze Zahl.' }]
```

- The first row is the header. Columns are found by `header` or one of the `aliases`, ignoring case and extra spaces; their order does not matter and other columns are ignored.
- A missing `required` column is reported as `MISSING_COLUMN` (row 1) and no rows are read.
- `rows` holds the rows without errors, as objects keyed like the schema, with values converted to the column `type`. Empty optional cells are `null`. Completely empty rows are skipped.
- `errors` holds one entry per rejected cell, with the row number as Excel shows it (header = row 1), the cell address and a German message for the user. Codes: `MISSING_COLUMN`, `REQUIRED`, `INVALID_TYPE`, `BELOW_MIN`, `ABOVE_MAX`, `PATTERN`, `NOT_ALLOWED`, `CUSTOM`. At most `maxErrors` (default 1,000) are collected; `errorsTruncated` tells if there were more.
- `rowNumbers[i]` is the Excel row number of `rows[i]` (the header is row 1), e.g. for a message like „steht schon in Zeile 7“ after a duplicate check.
- `rowCount` is the number of data rows, valid or not.

| `type` | Typed cells | Text cells (always in csv) |
|---|---|---|
| `string` (default) | numbers and dates as text | trimmed |
| `number`, `integer` | number cells | `1.234,56` or `1 234,56` (`decimal: ','`, default), `1,234.56` (`decimal: '.'`) |
| `date` | date cells | `31.12.2025`, `31.12.25`, `2025-12-31`, each optionally with `14:30[:00]`; impossible dates fail |
| `boolean` | boolean cells, 1/0 | `ja`/`nein`, `wahr`/`falsch`, `true`/`false`, `x` |

`min`/`max` bound the value (numbers; `Date` objects for dates) or the length (strings); invalid bounds throw `INVALID_INPUT`. `values` matches text ignoring case and returns the value as listed. `validate` runs last and returns a message (appended to the column name) to reject the value. An invalid schema throws `INVALID_INPUT` before the file is read, and so does a `validate` function that throws (that is a bug in the app, not in the file). Two-digit years follow Excel: `00`–`29` are 2000–2029, `30`–`99` are 1930–1999.

Declare a schema in its own variable with `satisfies Schema`, so the column types stay literal and the row type is inferred:

```ts
import { readSheet, type Schema } from '@10eputzen/excel-rs';

const orders = {
  qty: { header: 'Menge', type: 'integer', required: true },
} satisfies Schema;

const { rows } = readSheet(bytes, { schema: orders }); // rows: { qty: number }[]
```

## Checking files before upload

```ts
import { detectFormat, readCsvHeader, missingColumns } from '@10eputzen/excel-rs/detect';

const head = new Uint8Array(await file.slice(0, 4096).arrayBuffer());
const format = detectFormat(head); // 'xlsx' | 'ods' | 'xls' | 'csv' | 'unknown'

if (format === 'csv') {
  const { delimiter, columns } = readCsvHeader(await file.slice(0, 65536).text());
  const missing = missingColumns(columns, schema); // headers of required columns that are absent
}
```

`detectFormat` looks at the first bytes only: zip archives count as `xlsx` (xlsm and xlsb are zip archives too) unless they are ods; text without binary content counts as `csv`. It is a quick check for the user; the server still validates the whole file. `missingColumns` accepts the same schema object as `readSheet` and compares headers the same way.

## Writing

```ts
import { writeFileSync } from 'node:fs';
import { writeXlsx } from '@10eputzen/excel-rs';

const bytes = writeXlsx([
  {
    name: 'Orders',
    columns: [
      { header: 'Customer' },
      { header: 'Date', format: 'date' },
      { header: 'Amount', format: '#,##0.00', width: 14 },
      { header: 'Zip code', format: 'text' },
    ],
    rows: [
      ['ACME Corp.', new Date(Date.UTC(2024, 1, 29)), 1234.5, '01067'],
    ],
  },
]);
writeFileSync('orders.xlsx', bytes);
```

- `format`: `'text'`, `'date'` and `'datetime'` use Excel's built-in formats, which display in the reader's locale. Any other string is used as an Excel number format.
- Dates without a date column format get `date` or `datetime` automatically. Dates are written as UTC wall-clock time.
- Strings are always written as text, never as formulas, so values such as `=1+1` stay literal.
- `null`, `undefined`, `NaN`, `Infinity` and invalid dates become empty cells.
- Column widths fit the content (up to 80 characters) unless `width` is set.
- `freezeHeader` and `autoFilter` default to `true`. With an empty `columns` array no header row is written.

## Browser (deprecated)

`@10eputzen/excel-rs/browser` runs the full reader and writer in the browser. It is deprecated since 0.2.0 because it loads about 0.8 MB of WebAssembly (gzip): read files on the server and check them before the upload with [`@10eputzen/excel-rs/detect`](#checking-files-before-upload). It keeps working until a later major version removes it.

```ts
const { readSheet, writeXlsx, initExcel } = await import('@10eputzen/excel-rs/browser');
```

Same options (without `schema`), results and errors as in Node.js, but every function returns a promise. The `.wasm` is loaded from next to the bindings (Vite and webpack 5 bundle it); with other bundlers copy `node_modules/@10eputzen/excel-rs/wasm/web/excel_rs_wasm_bg.wasm` to your assets and call `initExcel(url)` first. A failed load rejects with `WASM_LOAD_FAILED` and is retried on the next call. The page needs `'wasm-unsafe-eval'` in `script-src`.

## Errors

Both functions throw `ExcelError` with a `code` (the browser entry rejects with it):

| code | meaning |
|---|---|
| `UNSUPPORTED_FORMAT` | content or `format` option is not a supported spreadsheet |
| `CORRUPT_FILE` | the file could not be parsed |
| `SHEET_NOT_FOUND` | the requested sheet does not exist |
| `TOO_LARGE` | a size limit was exceeded |
| `INVALID_INPUT` | invalid options or data for `writeXlsx` |
| `WASM_LOAD_FAILED` | browser entry only: the WebAssembly file could not be loaded |

```ts
import { ExcelError } from '@10eputzen/excel-rs';

try {
  readSheet(bytes);
} catch (err) {
  if (err instanceof ExcelError && err.code === 'TOO_LARGE') { /* … */ }
}
```

## Known limitations

- xlsb files that use compact cell records (written by some third-party tools, not by Excel) lose most cell values.
- Password-protected files are not supported (`CORRUPT_FILE`).
- Numbers are written with 15 significant digits, like Excel keeps them.
- No formulas, styles beyond the header row, images or charts when writing.
- The whole file is held in memory.
- Files from Google Sheets are not yet part of the test suite.

## Development

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130 --locked
cargo test
cd npm && npm ci && npm run build && npm test
npm run test:browser   # Chromium via Playwright
```

`npm run fixtures` regenerates the reference files in `fixtures/` (needs LibreOffice).

```sh
UPDATE_GOLDEN=1 cargo test -p excel-rs-core --test golden   # after an intended change; review the diff
PROPTEST_CASES=20000 cargo test --release --test properties  # longer property run
cargo +nightly fuzz run read                                 # fuzzing, see fuzz/; seed with fuzz/seed-corpus.sh
cd npm/bench && npm install && npm run bench                 # benchmarks, updates the README table
```

Releases: bump the version in `npm/package.json`, add the `CHANGELOG.md` entry, merge, then start the **Release** workflow. It publishes to npm with provenance and creates the tag and GitHub release.

## Contributing

Issues and pull requests are welcome, see [CONTRIBUTING.md](CONTRIBUTING.md). Please report security problems privately, see [SECURITY.md](SECURITY.md).

## Built on

[calamine](https://github.com/tafia/calamine), [rust_xlsxwriter](https://github.com/jmcnamara/rust_xlsxwriter), [csv](https://github.com/BurntSushi/rust-csv) and [encoding_rs](https://github.com/hsivonen/encoding_rs).

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this project, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
