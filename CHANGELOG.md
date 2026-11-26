# Changelog

## 0.3.0 – 2026-11-26

- First npm release, as `@10eputzen/excel-rs` (npm does not accept the unscoped name). Imports change from `excel-rs` to `@10eputzen/excel-rs`; the API is unchanged.
- `readSheet(…, { schema })` returns `rowNumbers`: the Excel row number of each valid row, e.g. to point at duplicates.
- csv: blank lines are kept as empty rows, so row numbers in errors and `rowNumbers` match what Excel shows (before, every blank line shifted the following rows up by one).
- csv: a file whose first line has no delimiter is read as a single column. Before, a German one-column file with `-0,01` was split at the comma and read as `-0`.
- xlsx, xlsb and ods must start with the zip signature; other content fails with `CORRUPT_FILE` (a crafted 10 KB file made the parser allocate 4 GiB).
- `writeXlsx` is about 7× faster for large sheets (100,000 rows: 11 s → 1.6 s): the WebAssembly heap is reserved up front instead of growing in small steps, each of which made V8 run a full garbage collection.
- Tests: files saved by Microsoft Excel with golden results, property tests, fuzzing (weekly in CI), CI on Windows, benchmarks against SheetJS, exceljs and read-excel-file in the README.

## 0.2.0 – 2026-11-26

- `readSheet(bytes, { schema })`: finds columns by header (aliases, ignoring case and spaces), converts cells to string, number, integer, date or boolean (German notation for text: `1.234,56`, `31.12.2025`, `ja`/`nein`), checks required, min/max, pattern, allowed values and own rules, and returns valid rows as typed objects plus errors with row, cell and a German message.
- New entry `excel-rs/detect` (pure TypeScript, under 2 KB gzip, no WebAssembly): `detectFormat`, `readCsvHeader`, `missingColumns` for checks in the browser before an upload.
- `excel-rs/browser` is deprecated; it stays until a later major version.

## 0.1.0 – unreleased

- `readSheet`: read xlsx, xlsm, xls, xlsb, ods and csv (delimiter and encoding detection, BOM, Windows-1252).
- `writeXlsx`: write xlsx with bold, frozen, filterable header, column formats and fitted widths.
- Browser entry `excel-rs/browser`: same API as promises, lazy-loaded WebAssembly, works under a CSP with `'wasm-unsafe-eval'`.
- `ExcelError` with stable codes; limits for input size, uncompressed size, rows and cells, enforced before allocation (record scan for xls and ods); WebAssembly memory capped at 2 GiB.
