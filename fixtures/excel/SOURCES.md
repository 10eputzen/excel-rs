# Files saved by Microsoft Excel

These workbooks were saved by Microsoft Excel (checked in `docProps/app.xml`
for xlsx/xlsb and in the summary stream for xls). They come from the test data
of [Apache POI](https://github.com/apache/poi/tree/f63b77bb7497d950073969f9d418f2270dd87edd/test-data/spreadsheet)
(commit `f63b77b`), licensed under the Apache License 2.0
([LICENSE-APACHE-2.0.txt](LICENSE-APACHE-2.0.txt)). They are unchanged.

`<file>.json` next to each workbook is the expected result of reading every
sheet; `crates/core/tests/golden.rs` compares against it. The values were
cross-checked against SheetJS 0.20.3: all cells agree except `InlineString.xlsx`
A1, where SheetJS drops the emoji of the first rich-text run.

| File | Saved by | Covers |
| --- | --- | --- |
| `SampleSS.xlsx`, `SampleSS.xls` | Excel 2007 / Excel 97–2003 | the same sample in both formats |
| `simple-monthly-budget.xlsx` | Excel 2016+ | template with formulas and number formats |
| `table-sample.xlsx` | Excel 2016+, German (sheet `Tabelle1`) | Excel table (ListObject) |
| `62629_toMerge.xlsx` | Excel 2016+ | wide sheet with many styles |
| `unicodeSheetName.xlsx`, `unicodeNameRecord.xls` | Excel | non-ASCII sheet and name records |
| `formula-eval.xlsx` | Excel Online | cached formula results |
| `InlineString.xlsx` | Excel (XML edited by hand) | inline rich text with an emoji |
| `DateFormatTests.xlsx` | Excel for Mac | 1904 date system, many date formats |
| `1900DateWindowing.xls`, `1904DateWindowing.xls` | Excel | both date systems in xls |
| `DateFormats.xls` | Excel | date formats in xls |
| `date.xlsb`, `sample.xlsb`, `testVarious.xlsb` | Excel 2016+ | binary workbook: dates, rich text, integers, floats, currency, percent, long numbers |

German content (umlauts, `;` CSV, Windows-1252, `1.234,56`) is covered by
`../reference-*` and `../german-excel.csv` and by the property tests in
`crates/core/tests/properties.rs`.
