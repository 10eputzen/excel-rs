use excel_rs_core::{read_sheet, write_xlsx_json, ReadOptions, SheetSelector, Value};

fn fixture(name: &str) -> Vec<u8> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/");
    std::fs::read(format!("{path}{name}")).expect("fixture")
}

#[test]
fn reads_reference_files() {
    for file in [
        "reference-sheetjs.xlsx",
        "reference-libreoffice.xlsx",
        "reference-libreoffice.xls",
        "reference-libreoffice.ods",
    ] {
        let data = read_sheet(&fixture(file), &ReadOptions::default()).unwrap();
        assert_eq!(data.sheet_names, ["Daten", "Zweite"], "{file}");
        assert_eq!(data.rows.len(), 4, "{file}");
        assert_eq!(
            data.rows[1][0],
            Value::String("Größe Äpfel ß €".into()),
            "{file}"
        );
        assert_eq!(data.rows[1][1], Value::Number(1234.56), "{file}");
        // 2024-02-29T00:00:00Z
        assert_eq!(data.rows[1][2], Value::Date(1_709_164_800_000.0), "{file}");
        assert_eq!(data.rows[1][3], Value::Bool(true), "{file}");
        assert_eq!(data.rows[1][4], Value::Number(2469.12), "{file}");
        assert_eq!(data.rows[3][5], Value::String("Ende".into()), "{file}");
    }
}

#[test]
fn selects_sheet_by_name() {
    let opts = ReadOptions {
        sheet: SheetSelector::Name("Zweite".into()),
        ..Default::default()
    };
    let data = read_sheet(&fixture("reference-libreoffice.xlsx"), &opts).unwrap();
    assert_eq!(
        data.rows,
        vec![vec![Value::String("x".into())], vec![Value::Number(1.0)]]
    );
}

#[test]
fn write_then_read() {
    let json = r#"[{"name":"S","columns":[{"header":"a"},{"header":"d","format":"date"}],
        "rows":[["ä",{"d":1709164800000}],[1.5,null]]}]"#;
    let bytes = write_xlsx_json(json).unwrap();
    let data = read_sheet(&bytes, &ReadOptions::default()).unwrap();
    assert_eq!(
        data.rows,
        vec![
            vec![Value::String("a".into()), Value::String("d".into())],
            vec![Value::String("ä".into()), Value::Date(1_709_164_800_000.0)],
            vec![Value::Number(1.5), Value::Empty],
        ]
    );
}

#[test]
fn truncated_input_is_an_error_not_a_panic() {
    // Known parser panic on this prefix; must surface as CORRUPT_FILE.
    let xls = fixture("reference-libreoffice.xls");
    let err = read_sheet(&xls[..513], &ReadOptions::default()).unwrap_err();
    assert_eq!(err.code, excel_rs_core::ErrorCode::CorruptFile);

    for file in [
        "reference-libreoffice.xlsx",
        "reference-libreoffice.xls",
        "reference-libreoffice.ods",
        "reference-sheetjs.xlsb",
    ] {
        let bytes = fixture(file);
        for len in (0..bytes.len()).step_by(97) {
            let _ = read_sheet(&bytes[..len], &ReadOptions::default());
        }
    }
}

#[test]
fn far_away_cell_is_rejected_before_allocation() {
    // Two cells, A1 and XFD5000: the dense result would have 82 million cells.
    let mut wb = rust_xlsxwriter::Workbook::new();
    let ws = wb.add_worksheet();
    ws.write_string(0, 0, "a").unwrap();
    ws.write_string(4999, 16_383, "z").unwrap();
    let bytes = wb.save_to_buffer().unwrap();
    assert!(bytes.len() < 10_000);
    let err = read_sheet(&bytes, &ReadOptions::default()).unwrap_err();
    assert_eq!(err.code, excel_rs_core::ErrorCode::TooLarge);
}
