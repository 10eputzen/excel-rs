//! Golden files: spreadsheets saved by Microsoft Excel (see
//! `fixtures/excel/SOURCES.md`), each with the expected result of reading every
//! sheet in `<file>.json`. A change in parsing shows up as a diff.
//!
//! Regenerate after an intended change, then review the diff:
//! `UPDATE_GOLDEN=1 cargo test -p excel-rs-core --test golden`

use std::path::Path;

use excel_rs_core::{read_sheet, ReadOptions, SheetSelector};

fn snapshot(bytes: &[u8]) -> String {
    let first = read_sheet(bytes, &ReadOptions::default()).expect("readable");
    let mut sheets = serde_json::Map::new();
    for (i, name) in first.sheet_names.iter().enumerate() {
        let opts = ReadOptions {
            sheet: SheetSelector::Index(i),
            ..Default::default()
        };
        let value = match read_sheet(bytes, &opts) {
            Ok(data) => serde_json::to_value(&data.rows).unwrap(),
            Err(e) => serde_json::json!({ "error": format!("{:?}", e.code) }),
        };
        sheets.insert(name.clone(), value);
    }
    let doc = serde_json::json!({ "sheetNames": first.sheet_names, "sheets": sheets });
    let mut out = String::new();
    // One row per line: readable diffs, small files.
    out.push_str("{\n  \"sheetNames\": ");
    out.push_str(&doc["sheetNames"].to_string());
    out.push_str(",\n  \"sheets\": {");
    let mut first_sheet = true;
    for (name, rows) in doc["sheets"].as_object().unwrap() {
        out.push_str(if first_sheet { "\n" } else { ",\n" });
        first_sheet = false;
        out.push_str(&format!(
            "    {}: ",
            serde_json::Value::String(name.clone())
        ));
        match rows.as_array() {
            Some(rows) if !rows.is_empty() => {
                out.push_str("[\n");
                let lines: Vec<String> = rows.iter().map(|r| format!("      {r}")).collect();
                out.push_str(&lines.join(",\n"));
                out.push_str("\n    ]");
            }
            _ => out.push_str(&rows.to_string()),
        }
    }
    out.push_str("\n  }\n}\n");
    out
}

#[test]
fn excel_files_match_golden_results() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/excel");
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            matches!(
                p.extension().and_then(|e| e.to_str()),
                Some("xlsx" | "xls" | "xlsb")
            )
        })
        .collect();
    files.sort();
    assert!(files.len() >= 10, "fixtures missing");
    let mut failed = Vec::new();
    for file in files {
        let got = snapshot(&std::fs::read(&file).unwrap());
        let golden = file.with_extension(format!(
            "{}.json",
            file.extension().unwrap().to_str().unwrap()
        ));
        if update {
            std::fs::write(&golden, &got).unwrap();
            continue;
        }
        // Normalize line endings: git may check the json out with CRLF on Windows.
        let want = std::fs::read_to_string(&golden)
            .unwrap_or_default()
            .replace("\r\n", "\n");
        if got != want {
            failed.push(file.file_name().unwrap().to_string_lossy().into_owned());
        }
    }
    assert!(
        failed.is_empty(),
        "differs from golden result: {failed:?} (UPDATE_GOLDEN=1 regenerates)"
    );
}
