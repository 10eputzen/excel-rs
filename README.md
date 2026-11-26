# excel-rs

Fast spreadsheet import and export for Node.js and browsers.

- **Read** xlsx, xlsm, xls, xlsb, ods and csv
- **Write** xlsx with a bold, frozen, filterable header row and column formats
- Rust core ([calamine](https://github.com/tafia/calamine), [rust_xlsxwriter](https://github.com/jmcnamara/rust_xlsxwriter), [csv](https://github.com/BurntSushi/rust-csv), [encoding_rs](https://github.com/hsivonen/encoding_rs)) compiled to WebAssembly
- **Zero runtime dependencies**, one platform-independent package (no native binaries, no postinstall)
- Typed errors with stable codes and limits against oversized or malicious files

```sh
npm install excel-rs
```

Requires Node.js 20 or newer, or a browser with WebAssembly and ES modules. Works in serverless environments that run Node.js.

| Entry | Use | API |
|---|---|---|
| `excel-rs` | Node.js (CommonJS and ESM) | synchronous |
| `excel-rs/browser` | browsers, bundlers, Deno, workers | `async`, loads the `.wasm` on first use |

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

## Browser

```ts
// Load lazily, only where import or export is used:
const { readSheet, writeXlsx } = await import('excel-rs/browser');

const file = input.files[0];
const { rows } = await readSheet(new Uint8Array(await file.arrayBuffer()));

const bytes = await writeXlsx([{ name: 'Export', columns: [{ header: 'Name' }], rows: [['ACME']] }]);
const url = URL.createObjectURL(new Blob([bytes], { type: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet' }));
```

Same options, results and errors as in Node.js, but every function returns a promise. The WebAssembly binary (about 0.8 MB gzip) is fetched on the first call.

**Where the `.wasm` comes from.** By default it is loaded from `excel_rs_wasm_bg.wasm` next to the bindings (`new URL(…, import.meta.url)`), which Vite and webpack 5 bundle as an asset automatically. If your bundler does not (e.g. esbuild-based builds), copy `node_modules/excel-rs/wasm/web/excel_rs_wasm_bg.wasm` into your static assets and point to it once before the first call:

```ts
import { initExcel } from 'excel-rs/browser';
await initExcel('/assets/excel_rs_wasm_bg.wasm'); // URL, Response, bytes or WebAssembly.Module
```

**Large files.** Parsing runs on the calling thread. For files with more than a few thousand rows, call excel-rs from a Web Worker so the page stays responsive, and set `maxBytes`/`maxRows` to what your app needs.

**Load errors.** If the `.wasm` cannot be fetched or compiled (wrong URL, an SPA fallback serving HTML), calls reject with `WASM_LOAD_FAILED`; the next call retries. `initExcel` with a different source after loading started rejects with `INVALID_INPUT`.

**Content Security Policy.** Instantiating WebAssembly needs `'wasm-unsafe-eval'` in `script-src` (not `'unsafe-eval'`).

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
