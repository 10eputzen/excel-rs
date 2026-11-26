//! Property tests: random sheets are written, read back and compared.
//! Focus on what German spreadsheets contain: umlauts, ß, €, `;` and `,`
//! inside cells, quotes, line breaks, blank rows, LF and CRLF, Windows-1252.

use excel_rs_core::{read_sheet, CsvEncoding, CsvOptions, ReadOptions, Value};
use proptest::prelude::*;
use serde_json::json;

/// Text cells from a German-heavy alphabet, including CSV special characters.
fn text() -> impl Strategy<Value = String> {
    let chars = prop::sample::select(vec![
        'a', 'Z', '0', '7', ' ', 'ä', 'ö', 'ü', 'Ä', 'Ö', 'Ü', 'ß', '€', '§', '°', ';', ',', '"',
        '\n', '\t', '.', '-', '\'', 'é',
    ]);
    prop::collection::vec(chars, 0..12).prop_map(|v| v.into_iter().collect())
}

/// A grid where some rows are left completely empty.
fn grid() -> impl Strategy<Value = Vec<Vec<String>>> {
    (1usize..6).prop_flat_map(|width| {
        prop::collection::vec(
            prop_oneof![
                4 => prop::collection::vec(text(), width),
                1 => Just(vec![String::new(); width]),
            ],
            1..20,
        )
    })
}

/// Serializes like Excel's "CSV (Trennzeichen-getrennt)": quote when needed.
fn to_csv(rows: &[Vec<String>], sep: char, eol: &str) -> String {
    let mut out = String::new();
    for row in rows {
        let cells: Vec<String> = row
            .iter()
            .map(|c| {
                if c.contains(sep) || c.contains('"') || c.contains('\n') || c.contains('\r') {
                    format!("\"{}\"", c.replace('"', "\"\""))
                } else {
                    c.clone()
                }
            })
            .collect();
        out.push_str(&cells.join(&sep.to_string()));
        out.push_str(eol);
    }
    out
}

/// What the reader must return: strings, empty cells as `Empty`, trailing
/// empty rows dropped, every row padded to the widest one.
fn expected(rows: &[Vec<String>]) -> Vec<Vec<Value>> {
    let mut out: Vec<Vec<Value>> = rows
        .iter()
        .map(|r| {
            r.iter()
                .map(|c| {
                    if c.is_empty() {
                        Value::Empty
                    } else {
                        Value::String(c.clone())
                    }
                })
                .collect()
        })
        .collect();
    while out
        .last()
        .is_some_and(|r| r.iter().all(|v| *v == Value::Empty))
    {
        out.pop();
    }
    out
}

fn csv_opts(sep: u8, encoding: CsvEncoding) -> ReadOptions {
    ReadOptions {
        csv: CsvOptions {
            delimiter: Some(sep),
            encoding,
        },
        ..Default::default()
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Every cell, blank row and row number survives a CSV round trip.
    #[test]
    fn csv_round_trip(rows in grid(), semicolon in any::<bool>(), crlf in any::<bool>(), bom in any::<bool>()) {
        let (sep, sep_byte) = if semicolon { (';', b';') } else { (',', b',') };
        let text = to_csv(&rows, sep, if crlf { "\r\n" } else { "\n" });
        // Excel itself writes CRLF inside quoted cells as LF; keep `\n` there.
        let mut bytes = if bom { vec![0xEF, 0xBB, 0xBF] } else { Vec::new() };
        bytes.extend_from_slice(text.as_bytes());
        // A `.csv` file is detected as CSV unless it is empty.
        prop_assume!(!text.trim().is_empty() || !expected(&rows).is_empty());
        let data = read_sheet(&bytes, &csv_opts(sep_byte, CsvEncoding::Utf8));
        let want = expected(&rows);
        match data {
            Ok(d) => {
                let got: Vec<Vec<Value>> = d.rows.into_iter()
                    .map(|mut r| { r.truncate(rows[0].len()); r })
                    .collect();
                prop_assert_eq!(got, want);
            }
            Err(e) => prop_assert!(want.is_empty(), "error {e:?} for {text:?}"),
        }
    }

    /// The same for Excel's ANSI export (Windows-1252), detected automatically.
    #[test]
    fn windows_1252_round_trip(rows in grid()) {
        let text = to_csv(&rows, ';', "\r\n");
        let (bytes, _, lossy) = encoding_rs::WINDOWS_1252.encode(&text);
        prop_assume!(!lossy);
        // Some byte pairs are valid in both encodings ("Ä€" = C4 80 = "Ā" in
        // UTF-8). Detection then picks UTF-8, by design; callers who know the
        // file is ANSI pass the encoding. Only unambiguous input is checked.
        prop_assume!(bytes.is_ascii() || std::str::from_utf8(&bytes).is_err());
        let want = expected(&rows);
        prop_assume!(!want.is_empty());
        let d = read_sheet(&bytes, &csv_opts(b';', CsvEncoding::Auto)).unwrap();
        let got: Vec<Vec<Value>> = d.rows.into_iter()
            .map(|mut r| { r.truncate(rows[0].len()); r })
            .collect();
        prop_assert_eq!(got, want);
    }

    /// Values written with `write_xlsx_json` read back unchanged.
    #[test]
    fn xlsx_round_trip(cells in prop::collection::vec(prop::collection::vec(cell(), 1..6), 1..30)) {
        let width = cells.iter().map(Vec::len).max().unwrap();
        let rows: Vec<serde_json::Value> = cells.iter()
            .map(|r| serde_json::Value::Array(r.iter().map(|(j, _)| j.clone()).collect()))
            .collect();
        let json = json!([{ "name": "Blatt ä", "columns": [], "rows": rows }]).to_string();
        let bytes = excel_rs_core::write_xlsx_json(&json).unwrap();
        let d = read_sheet(&bytes, &ReadOptions::default()).unwrap();
        let mut want: Vec<Vec<Value>> = cells.iter()
            .map(|r| {
                let mut v: Vec<Value> = r.iter().map(|(_, v)| v.clone()).collect();
                v.resize(width, Value::Empty);
                v
            })
            .collect();
        while want.last().is_some_and(|r| r.iter().all(|v| *v == Value::Empty)) {
            want.pop();
        }
        // The reader trims empty outer columns; compare the used width only.
        let got: Vec<Vec<Value>> = d.rows.into_iter().map(|mut r| { r.resize(width, Value::Empty); r }).collect();
        prop_assert_eq!(d.sheet_names, vec!["Blatt ä".to_string()]);
        prop_assert_eq!(got.len(), want.len());
        for (g, w) in got.iter().flatten().zip(want.iter().flatten()) {
            prop_assert!(same(g, w), "{g:?} != {w:?}");
        }
    }
}

/// A cell as written (JSON) and as it must be read back.
fn cell() -> impl Strategy<Value = (serde_json::Value, Value)> {
    prop_oneof![
        1 => Just((serde_json::Value::Null, Value::Empty)),
        3 => text().prop_filter("non-empty", |s| !s.is_empty())
            .prop_map(|s| (json!(s), Value::String(s))),
        2 => (-1e12f64..1e12).prop_map(|n| (json!(n), Value::Number(n))),
        1 => any::<i32>().prop_map(|n| (json!(n), Value::Number(f64::from(n)))),
        1 => any::<bool>().prop_map(|b| (json!(b), Value::Bool(b))),
        // 1900-03-01 .. 2199-12-31, whole seconds.
        2 => (-2_203_891_200i64..7_258_118_400).prop_map(|s| {
            let ms = (s * 1000) as f64;
            (json!({ "d": ms }), Value::Date(ms))
        }),
    ]
}

/// Equal, with numbers compared to 15 significant digits: Excel itself keeps
/// no more, and the writer rounds the 17th digit.
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            x == y || (x - y).abs() <= 1e-15 * x.abs().max(y.abs())
        }
        _ => a == b,
    }
}
