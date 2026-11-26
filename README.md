# excel-rs

Fast spreadsheet import and export for Node.js.

- **Read** xlsx, xlsm, xls, xlsb, ods and csv
- **Validate** rows against a schema, with error messages in German
- **Write** xlsx with a bold, frozen, filterable header row and column formats
- **Check before upload** in the browser with a tiny entry (under 2 KB gzip, no WebAssembly)
- Rust core ([calamine](https://github.com/tafia/calamine), [rust_xlsxwriter](https://github.com/jmcnamara/rust_xlsxwriter), [csv](https://github.com/BurntSushi/rust-csv), [encoding_rs](https://github.com/hsivonen/encoding_rs)) compiled to WebAssembly
- **Zero runtime dependencies**, one platform-independent package (no native binaries, no postinstall)
- Typed errors with stable codes and limits against oversized or malicious files

```sh
npm install excel-rs
```

Requires Node.js 20 or newer. Works in serverless environments that run Node.js.

| Entry | Use | API |
|---|---|---|
| `excel-rs` | Node.js (CommonJS and ESM) | synchronous |
| `excel-rs/detect` | browsers: check a file before uploading it | pure TypeScript, under 2 KB gzip |
| `excel-rs/browser` | deprecated, see [Browser](#browser-deprecated) | `async`, loads 0.8 MB of WebAssembly |

The intended setup for web apps: the browser checks the file with `excel-rs/detect` and uploads it; the server reads it with `readSheet` and a schema, shows the user a preview with the errors, and writes only after confirmation.

## Reading

```ts
import { readFileSync } from 'node:fs';
import { readSheet } from 'excel-rs';

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

- UTF-8 and UTF-16 byte order marks are removed; text that is not valid UTF-8 is decoded as Windows-1252.
- The delimiter is the candidate (`;`, `,`, tab, `|`) that appears most consistently in the first lines, ignoring quoted text.

CSV cells are always strings (or `null` when empty). Number and date formats in CSV depend on the locale of the exporting program, so the conversion is left to you.

## Validating with a schema

```ts
import { readSheet } from 'excel-rs';

const { rows, errors, rowCount } = readSheet(bytes, {
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
import { readSheet, type Schema } from 'excel-rs';

const orders = {
  qty: { header: 'Menge', type: 'integer', required: true },
} satisfies Schema;

const { rows } = readSheet(bytes, { schema: orders }); // rows: { qty: number }[]
```

## Checking files before upload

```ts
import { detectFormat, readCsvHeader, missingColumns } from 'excel-rs/detect';

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
import { writeXlsx } from 'excel-rs';

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

`excel-rs/browser` runs the full reader and writer in the browser. It is deprecated since 0.2.0 because it loads about 0.8 MB of WebAssembly (gzip): read files on the server and check them before the upload with [`excel-rs/detect`](#checking-files-before-upload). It keeps working until a later major version removes it.

```ts
const { readSheet, writeXlsx, initExcel } = await import('excel-rs/browser');
```

Same options (without `schema`), results and errors as in Node.js, but every function returns a promise. The `.wasm` is loaded from next to the bindings (Vite and webpack 5 bundle it); with other bundlers copy `node_modules/excel-rs/wasm/web/excel_rs_wasm_bg.wasm` to your assets and call `initExcel(url)` first. A failed load rejects with `WASM_LOAD_FAILED` and is retried on the next call. The page needs `'wasm-unsafe-eval'` in `script-src`.

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
import { ExcelError } from 'excel-rs';

try {
  readSheet(bytes);
} catch (err) {
  if (err instanceof ExcelError && err.code === 'TOO_LARGE') { /* … */ }
}
```

## Known limitations

- xlsb files that use compact cell records (written by some third-party tools) lose most cell values. Files written by Excel are expected to work but are not yet covered by the test suite.
- No formulas, styles beyond the header row, images or charts when writing.
- The whole file is held in memory.
- Tested with files written by LibreOffice, SheetJS and this package. Files from Excel and Google Sheets are not yet part of the test suite.

## Development

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130 --locked
cargo test
cd npm && npm ci && npm run build && npm test
npm run test:browser   # Chromium via Playwright
```

`npm run fixtures` regenerates the reference files in `fixtures/` (needs LibreOffice).

## License

MIT
