//! Small files that would make the parser allocate gigabytes must fail with
//! TOO_LARGE before that happens.

use std::io::{Cursor, Read, Write};

use excel_rs_core::{read_sheet, ErrorCode, ReadOptions};
use zip::write::SimpleFileOptions;
use zip::CompressionMethod;

fn fixture(name: &str) -> Vec<u8> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/");
    std::fs::read(format!("{path}{name}")).expect("fixture")
}

/// Copies a zip archive, replacing one entry.
fn replace_entry(archive: &[u8], name: &str, content: &[u8]) -> Vec<u8> {
    let mut src = zip::ZipArchive::new(Cursor::new(archive)).unwrap();
    let mut out = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for i in 0..src.len() {
        let mut entry = src.by_index(i).unwrap();
        let entry_name = entry.name().unwrap().into_owned();
        let mut data = Vec::new();
        entry.read_to_end(&mut data).unwrap();
        out.start_file(&entry_name, opts).unwrap();
        out.write_all(if entry_name == name { content } else { &data })
            .unwrap();
    }
    out.finish().unwrap().into_inner()
}

fn assert_too_large(bytes: &[u8], opts: &ReadOptions) {
    let err = read_sheet(bytes, opts).unwrap_err();
    assert_eq!(err.code, ErrorCode::TooLarge, "{}", err.message);
}

#[test]
fn xlsx_repeating_one_cell() {
    let mut xml = String::from(
        r#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1">"#,
    );
    for _ in 0..5_000 {
        xml.push_str(r#"<c r="A1"><v>1</v></c>"#);
    }
    xml.push_str("</row></sheetData></worksheet>");
    let bytes = replace_entry(
        &fixture("reference-libreoffice.xlsx"),
        "xl/worksheets/sheet1.xml",
        xml.as_bytes(),
    );
    let opts = ReadOptions {
        max_cells: 1_000,
        ..Default::default()
    };
    assert_too_large(&bytes, &opts);
}

#[test]
fn ods_repeated_rows_and_columns() {
    let xml = r#"<?xml version="1.0"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"><office:body><office:spreadsheet>
<table:table table:name="Daten"><table:table-row table:number-rows-repeated="1000000"><table:table-cell table:number-columns-repeated="16384" office:value-type="float" office:value="1"><text:p>1</text:p></table:table-cell></table:table-row></table:table>
</office:spreadsheet></office:body></office:document-content>"#;
    let bytes = replace_entry(
        &fixture("reference-libreoffice.ods"),
        "content.xml",
        xml.as_bytes(),
    );
    assert!(bytes.len() < 50_000);
    assert_too_large(&bytes, &ReadOptions::default());
}

#[test]
fn ods_trailing_empty_repeats_are_fine() {
    // LibreOffice writes huge repeat counts for the empty rest of a sheet.
    let xml = r#"<?xml version="1.0"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"><office:body><office:spreadsheet>
<table:table table:name="Daten"><table:table-row><table:table-cell office:value-type="string"><text:p>a</text:p></table:table-cell><table:table-cell table:number-columns-repeated="16383"/></table:table-row><table:table-row table:number-rows-repeated="1048575"><table:table-cell table:number-columns-repeated="16384"/></table:table-row></table:table>
</office:spreadsheet></office:body></office:document-content>"#;
    let bytes = replace_entry(
        &fixture("reference-libreoffice.ods"),
        "content.xml",
        xml.as_bytes(),
    );
    let data = read_sheet(&bytes, &ReadOptions::default()).unwrap();
    assert_eq!(data.rows.len(), 1);
}

#[test]
fn xls_far_away_cell() {
    // Minimal BIFF8 stream: BOF, Number at row 65535 / column 255, EOF.
    let mut stream = Vec::new();
    let mut record = |typ: u16, data: &[u8]| {
        stream.extend_from_slice(&typ.to_le_bytes());
        stream.extend_from_slice(&(data.len() as u16).to_le_bytes());
        stream.extend_from_slice(data);
    };
    record(
        0x0809,
        &[0x00, 0x06, 0x10, 0x00, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    );
    let mut number = vec![0xFF, 0xFF, 0xFF, 0x00, 0, 0];
    number.extend_from_slice(&1.0f64.to_le_bytes());
    record(0x0203, &number);
    record(0x000A, &[]);
    let mut cfb = cfb::CompoundFile::create(Cursor::new(Vec::new())).unwrap();
    cfb.create_stream("/Workbook")
        .unwrap()
        .write_all(&stream)
        .unwrap();
    let bytes = cfb.into_inner().into_inner();
    assert_too_large(&bytes, &ReadOptions::default());
}
