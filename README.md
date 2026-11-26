# excel-rs

Fast spreadsheet import and export for Node.js.

- **Read** xlsx, xlsm, xls, xlsb, ods and csv
- **Write** xlsx with a bold, frozen, filterable header row and column formats
- Rust core ([calamine](https://github.com/tafia/calamine), [rust_xlsxwriter](https://github.com/jmcnamara/rust_xlsxwriter), [csv](https://github.com/BurntSushi/rust-csv), [encoding_rs](https://github.com/hsivonen/encoding_rs)) compiled to WebAssembly
- **Zero runtime dependencies**, one platform-independent package (no native binaries, no postinstall)
- Typed errors with stable codes and limits against oversized or malicious files

```sh
npm install excel-rs
```

Requires Node.js 20 or newer. Works in serverless environments that run Node.js.

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

Invalid option values throw `INVALID_INPUT`. For xlsx and xlsb the row and cell limits are enforced while reading, before the result is allocated. For xls and ods the parser allocates the sheet first; the WebAssembly memory is capped at 2 GiB, so a malicious file fails with `CORRUPT_FILE` instead of exhausting the process. After a call that grew the WebAssembly memory beyond 256 MiB, the module is reloaded so that the memory is released.

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

## Errors

Both functions throw `ExcelError` with a `code`:

| code | meaning |
|---|---|
| `UNSUPPORTED_FORMAT` | content or `format` option is not a supported spreadsheet |
| `CORRUPT_FILE` | the file could not be parsed |
| `SHEET_NOT_FOUND` | the requested sheet does not exist |
| `TOO_LARGE` | a size limit was exceeded |
| `INVALID_INPUT` | invalid options or data for `writeXlsx` |

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
```

`npm run fixtures` regenerates the reference files in `fixtures/` (needs LibreOffice).

## License

MIT
