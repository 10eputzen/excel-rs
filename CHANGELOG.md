# Changelog

## 0.2.0 – 2026-11-26

- `readSheet(bytes, { schema })`: finds columns by header (aliases, ignoring case and spaces), converts cells to string, number, integer, date or boolean (German notation for text: `1.234,56`, `31.12.2025`, `ja`/`nein`), checks required, min/max, pattern, allowed values and own rules, and returns valid rows as typed objects plus errors with row, cell and a German message.
- New entry `excel-rs/detect` (pure TypeScript, under 2 KB gzip, no WebAssembly): `detectFormat`, `readCsvHeader`, `missingColumns` for checks in the browser before an upload.
- `excel-rs/browser` is deprecated; it stays until a later major version.

## 0.1.0 – unreleased

- `readSheet`: read xlsx, xlsm, xls, xlsb, ods and csv (delimiter and encoding detection, BOM, Windows-1252).
- `writeXlsx`: write xlsx with bold, frozen, filterable header, column formats and fitted widths.
- Browser entry `excel-rs/browser`: same API as promises, lazy-loaded WebAssembly, works under a CSP with `'wasm-unsafe-eval'`.
- `ExcelError` with stable codes; limits for input size, uncompressed size, rows and cells, enforced before allocation (record scan for xls and ods); WebAssembly memory capped at 2 GiB.
