//! WebAssembly bindings. Data crosses the boundary as JSON strings and byte
//! arrays; errors are thrown as JSON strings `{"code": …, "message": …}` that
//! the TypeScript wrapper turns into `ExcelError`.

use excel_rs_core::{
    read_sheet as core_read, write_xlsx_json, CsvEncoding, CsvOptions, Error, Format, ReadOptions,
    SheetSelector,
};
use wasm_bindgen::prelude::*;

/// Reads one sheet. `opts_json` is the serialized read options.
#[wasm_bindgen(js_name = readSheet)]
pub fn read_sheet(bytes: &[u8], opts_json: &str) -> Result<String, JsValue> {
    let run = || -> Result<String, Error> {
        let opts = parse_read_options(opts_json)?;
        let data = core_read(bytes, &opts)?;
        serde_json::to_string(&data).map_err(|e| Error::invalid(e.to_string()))
    };
    run().map_err(|e| JsValue::from_str(&e.to_json()))
}

/// Writes an xlsx workbook from the serialized sheet list.
#[wasm_bindgen(js_name = writeXlsx)]
pub fn write_xlsx(sheets_json: &str) -> Result<Vec<u8>, JsValue> {
    // The parsed JSON and the workbook take about 10× the JSON text.
    reserve_heap(sheets_json.len().saturating_mul(12));
    write_xlsx_json(sheets_json).map_err(|e| JsValue::from_str(&e.to_json()))
}

/// Grows the heap once to about `bytes` by allocating and freeing a block;
/// the allocator keeps the freed block and serves later allocations from it.
/// Without this, the heap grows in many small steps, and every growth of the
/// WebAssembly memory makes V8 run a full garbage collection of the
/// JavaScript heap, which for a large `rows` array costs seconds.
/// A failed reservation is ignored: the call then grows the heap as usual.
fn reserve_heap(bytes: usize) {
    let mut block: Vec<u8> = Vec::new();
    if block.try_reserve_exact(bytes.min(1 << 30)).is_ok() {
        drop(std::hint::black_box(block));
    }
}

/// Current size of the WebAssembly memory in bytes. The wrapper drops the
/// instance after large reads, because WebAssembly memory never shrinks.
#[wasm_bindgen(js_name = memoryBytes)]
pub fn memory_bytes() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        (core::arch::wasm32::memory_size(0) * 65_536) as f64
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        0.0
    }
}

/// Parses the options object built by the TypeScript wrapper.
pub fn parse_read_options(json: &str) -> Result<ReadOptions, Error> {
    use serde_json::Value as J;
    let v: J = serde_json::from_str(json).map_err(|e| Error::invalid(e.to_string()))?;
    let mut opts = ReadOptions::default();
    if let Some(f) = v.get("format").and_then(J::as_str) {
        opts.format = Format::parse(f)?;
    }
    match v.get("sheet") {
        None | Some(J::Null) => {}
        Some(J::String(s)) => opts.sheet = SheetSelector::Name(s.clone()),
        Some(J::Number(n)) => {
            let i = n
                .as_u64()
                .ok_or_else(|| Error::invalid("sheet index must be a non-negative integer"))?;
            let i = usize::try_from(i).map_err(|_| Error::invalid("sheet index is too large"))?;
            opts.sheet = SheetSelector::Index(i);
        }
        Some(_) => return Err(Error::invalid("sheet must be a name or an index")),
    }
    if let Some(csv) = v.get("csv") {
        opts.csv = parse_csv_options(csv)?;
    }
    let limit = |key: &str| -> Result<Option<u64>, Error> {
        match v.get(key) {
            None | Some(J::Null) => Ok(None),
            Some(x) => x
                .as_u64()
                .map(Some)
                .ok_or_else(|| Error::invalid(format!("{key} must be a non-negative integer"))),
        }
    };
    // Saturate instead of truncating on 32-bit targets.
    let to_usize = |n: u64| usize::try_from(n).unwrap_or(usize::MAX);
    if let Some(n) = limit("maxBytes")? {
        opts.max_bytes = to_usize(n);
    }
    if let Some(n) = limit("maxUncompressedBytes")? {
        opts.max_uncompressed_bytes = n;
    }
    if let Some(n) = limit("maxRows")? {
        opts.max_rows = to_usize(n);
    }
    if let Some(n) = limit("maxCells")? {
        opts.max_cells = to_usize(n);
    }
    Ok(opts)
}

fn parse_csv_options(v: &serde_json::Value) -> Result<CsvOptions, Error> {
    let mut csv = CsvOptions::default();
    match v.get("delimiter").and_then(|d| d.as_str()) {
        None | Some("auto") => {}
        Some(d) if d.len() == 1 && d.is_ascii() => csv.delimiter = Some(d.as_bytes()[0]),
        Some(d) => return Err(Error::invalid(format!("unsupported delimiter '{d}'"))),
    }
    csv.encoding = match v.get("encoding").and_then(|e| e.as_str()) {
        None | Some("auto") => CsvEncoding::Auto,
        Some("utf-8") | Some("utf8") => CsvEncoding::Utf8,
        Some("windows-1252") | Some("latin1") => CsvEncoding::Windows1252,
        Some(e) => return Err(Error::invalid(format!("unsupported encoding '{e}'"))),
    };
    Ok(csv)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options() {
        let o = parse_read_options(
            r#"{"format":"csv","sheet":"B","csv":{"delimiter":";","encoding":"windows-1252"},"maxRows":5}"#,
        )
        .unwrap();
        assert_eq!(o.format, Some(Format::Csv));
        assert_eq!(o.sheet, SheetSelector::Name("B".into()));
        assert_eq!(o.csv.delimiter, Some(b';'));
        assert_eq!(o.csv.encoding, CsvEncoding::Windows1252);
        assert_eq!(o.max_rows, 5);
        assert!(parse_read_options(r#"{"sheet":-1}"#).is_err());
        assert!(parse_read_options(r#"{"format":"pdf"}"#).is_err());
    }
}
