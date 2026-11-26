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

/// Minimal BIFF8 workbook: globals with one BoundSheet8 per entry of
/// `sheets` (all pointing at the same worksheet), then the worksheet records.
fn xls(sheets: usize, worksheet: &[(u16, Vec<u8>)]) -> Vec<u8> {
    compound(&[("/Workbook", &xls_stream(sheets, worksheet))])
}

/// A compound file with the given streams, created in this order.
fn compound(streams: &[(&str, &[u8])]) -> Vec<u8> {
    let mut cfb = cfb::CompoundFile::create(Cursor::new(Vec::new())).unwrap();
    for (path, data) in streams {
        if let Some((dir, _)) = path.rsplit_once('/').filter(|(d, _)| !d.is_empty()) {
            cfb.create_storage_all(dir).unwrap();
        }
        cfb.create_stream(path).unwrap().write_all(data).unwrap();
    }
    cfb.into_inner().into_inner()
}

fn xls_stream(sheets: usize, worksheet: &[(u16, Vec<u8>)]) -> Vec<u8> {
    let record = |out: &mut Vec<u8>, typ: u16, data: &[u8]| {
        out.extend_from_slice(&typ.to_le_bytes());
        out.extend_from_slice(&(data.len() as u16).to_le_bytes());
        out.extend_from_slice(data);
    };
    let bof = [0x00, 0x06, 0x05, 0x00, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    // BoundSheet8: offset (patched below), state, type, name length 1, flags, "S"
    let bound_sheet = |offset: u32| {
        let mut d = offset.to_le_bytes().to_vec();
        d.extend_from_slice(&[0, 0, 1, 0, b'S']);
        d
    };
    let globals_len = 4 + bof.len() + sheets * (4 + bound_sheet(0).len()) + 4;
    let mut stream = Vec::new();
    record(&mut stream, 0x0809, &bof);
    for _ in 0..sheets {
        record(&mut stream, 0x0085, &bound_sheet(globals_len as u32));
    }
    record(&mut stream, 0x000A, &[]);
    assert_eq!(stream.len(), globals_len);
    let mut sheet_bof = bof;
    sheet_bof[2] = 0x10;
    record(&mut stream, 0x0809, &sheet_bof);
    for (typ, data) in worksheet {
        record(&mut stream, *typ, data);
    }
    record(&mut stream, 0x000A, &[]);
    stream
}

fn number(row: u16, col: u16) -> (u16, Vec<u8>) {
    let mut d = row.to_le_bytes().to_vec();
    d.extend_from_slice(&col.to_le_bytes());
    d.extend_from_slice(&[0, 0]);
    d.extend_from_slice(&1.0f64.to_le_bytes());
    (0x0203, d)
}

#[test]
fn xls_far_away_cell() {
    // 65536 × 256 cells from two records.
    let bytes = xls(1, &[number(0, 0), number(0xFFFF, 0xFF)]);
    assert_too_large(&bytes, &ReadOptions::default());
}

#[test]
fn xls_dimensions_record() {
    // Dimensions claiming 1,000,000 × 255 cells: calamine reserves that up front.
    let mut d = 0u32.to_le_bytes().to_vec();
    d.extend_from_slice(&1_000_000u32.to_le_bytes());
    d.extend_from_slice(&0u16.to_le_bytes());
    d.extend_from_slice(&255u16.to_le_bytes());
    d.extend_from_slice(&[0, 0]);
    let bytes = xls(1, &[(0x0200, d)]);
    assert_too_large(&bytes, &ReadOptions::default());
}

#[test]
fn xls_dimensions_start_after_end() {
    // First row 10, last row 5: calamine's size computation wraps around.
    let mut d = 10u32.to_le_bytes().to_vec();
    d.extend_from_slice(&5u32.to_le_bytes());
    d.extend_from_slice(&0u16.to_le_bytes());
    d.extend_from_slice(&1u16.to_le_bytes());
    d.extend_from_slice(&[0, 0]);
    let bytes = xls(1, &[(0x0200, d)]);
    assert_too_large(&bytes, &ReadOptions::default());
}

#[test]
fn xls_workbook_stream_chosen_like_calamine() {
    // calamine takes the first directory entry named Workbook, here the one in a
    // storage. The prescan must check that stream, not the one at the root.
    let nested = xls_stream(1, &[number(0, 0), number(0xFFFF, 0xFF)]);
    let root = xls_stream(1, &[number(0, 0)]);
    let bytes = compound(&[("/A/Workbook", &nested), ("/Workbook", &root)]);
    assert_too_large(&bytes, &ReadOptions::default());

    let nested = xls_stream(1, &[number(0, 0), number(3, 0)]);
    let bytes = compound(&[("/A/Workbook", &nested), ("/Workbook", &root)]);
    let sheet = read_sheet(&bytes, &ReadOptions::default()).unwrap();
    assert_eq!(sheet.rows.len(), 4, "calamine read the nested stream");
}

#[test]
fn xls_cyclic_directory_chain() {
    let mut bytes = xls(1, &[number(0, 0)]);
    let u32_at = |b: &[u8], i: usize| u32::from_le_bytes(b[i..i + 4].try_into().unwrap());
    let dir_start = u32_at(&bytes, 48);
    let first_fat = u32_at(&bytes, 76) as usize;
    let sector = 1usize << u16::from_le_bytes([bytes[30], bytes[31]]);
    let entry = sector + first_fat * sector + dir_start as usize * 4;
    assert_eq!(
        u32_at(&bytes, entry),
        0xFFFF_FFFE,
        "end of the directory chain"
    );
    bytes[entry..entry + 4].copy_from_slice(&dir_start.to_le_bytes());
    let err = read_sheet(&bytes, &ReadOptions::default()).unwrap_err();
    assert_eq!(err.code, ErrorCode::CorruptFile, "{}", err.message);
}

#[test]
fn xls_fat_sector_listed_twice() {
    // Repeating FAT sectors in the DIFAT would multiply calamine's FAT table.
    let mut bytes = xls(1, &[number(0, 0)]);
    let first = bytes[76..80].to_vec();
    bytes[80..84].copy_from_slice(&first);
    let err = read_sheet(&bytes, &ReadOptions::default()).unwrap_err();
    assert_eq!(err.code, ErrorCode::CorruptFile, "{}", err.message);
}

#[test]
fn xls_unreadable_workbook_does_not_fall_back_to_book() {
    let stream = xls_stream(1, &[number(0, 0)]);
    let mut bytes = compound(&[("/Workbook", &stream), ("/Book", &stream)]);
    let name: Vec<u8> = "Workbook"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    let entry = bytes
        .windows(name.len())
        .position(|w| w == name.as_slice())
        .unwrap();
    bytes[entry + 116..entry + 120].copy_from_slice(&0xFFFF_FF00u32.to_le_bytes());
    let err = read_sheet(&bytes, &ReadOptions::default()).unwrap_err();
    assert_eq!(err.code, ErrorCode::CorruptFile, "{}", err.message);
}

#[test]
fn xls_sheet_listed_many_times() {
    // 2,000 × 100 cells is fine once, but 100 BoundSheet8 entries point at it.
    let bytes = xls(100, &[number(0, 0), number(1_999, 99)]);
    assert_too_large(&bytes, &ReadOptions::default());
    let once = xls(1, &[number(0, 0), number(1_999, 99)]);
    let err = read_sheet(&once, &ReadOptions::default()).err();
    assert!(
        err.as_ref().is_none_or(|e| e.code != ErrorCode::TooLarge),
        "{err:?}"
    );
}

#[test]
fn xls_not_a_compound_file() {
    let mut bytes = vec![0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
    bytes.extend_from_slice(&[0; 600]);
    let err = read_sheet(&bytes, &ReadOptions::default()).unwrap_err();
    assert_eq!(err.code, ErrorCode::CorruptFile);
}

#[test]
fn ods_invalid_or_duplicate_repeat_attribute() {
    let doc = |row_attrs: &str| {
        format!(
            r#"<?xml version="1.0"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"><office:body><office:spreadsheet>
<table:table table:name="Daten"><table:table-row {row_attrs}><table:table-cell table:number-columns-repeated="16384" office:value-type="float" office:value="1"><text:p>1</text:p></table:table-cell></table:table-row></table:table>
</office:spreadsheet></office:body></office:document-content>"#
        )
    };
    let ods = fixture("reference-libreoffice.ods");
    // Unparseable count: rejected, not read as 1.
    let bytes = replace_entry(
        &ods,
        "content.xml",
        doc(r#"table:number-rows-repeated="1e6""#).as_bytes(),
    );
    assert_eq!(
        read_sheet(&bytes, &ReadOptions::default())
            .unwrap_err()
            .code,
        ErrorCode::CorruptFile
    );
    // Duplicate attribute: the first one counts, as in the parser.
    let dup = r#"table:number-rows-repeated="1000000" table:number-rows-repeated="1""#;
    let bytes = replace_entry(&ods, "content.xml", doc(dup).as_bytes());
    assert_too_large(&bytes, &ReadOptions::default());
    // Entity-encoded digits are decoded before counting.
    let bytes = replace_entry(
        &ods,
        "content.xml",
        doc(r#"table:number-rows-repeated="&#49;000000""#).as_bytes(),
    );
    assert_too_large(&bytes, &ReadOptions::default());
}
