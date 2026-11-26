# Broken files found by fuzzing

Each file made a parser panic. `read_sheet` must report `CORRUPT_FILE`, and
the WebAssembly module must keep working afterwards
(`npm/test/robustness.test.mjs`).

| File | Found | Cause |
| --- | --- | --- |
| `xlsb-missing-relationship.xlsb` | 2026-11 | calamine 0.38 panics when a sheet entry in `workbook.bin` points to a relationship id that does not exist |
| `ole-sector-cycle.xlsx` | 2026-11 | an OLE container (not a zip) with a cyclic sector chain; read as xlsx, calamine 0.38 allocated 4 GiB. Zip-based formats now require the zip signature first |
