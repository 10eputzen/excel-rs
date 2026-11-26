use std::io::Cursor;

use calamine::{Data, DataRef, Ods, Range, Reader, Xls, Xlsb, Xlsx};

use crate::csv_read::{read_csv, CsvOptions};
use crate::detect::{check_zip_size, detect, Format};
use crate::error::{Error, ErrorCode, Result};
use crate::value::{civil_to_unix_ms, parse_iso_datetime, Value};

/// Which worksheet to read.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum SheetSelector {
    #[default]
    First,
    Name(String),
    Index(usize),
}

/// Options for [`read_sheet`]. Limits protect against oversized and malicious files.
#[derive(Debug, Clone)]
pub struct ReadOptions {
    /// `None` = detect from content.
    pub format: Option<Format>,
    pub sheet: SheetSelector,
    pub csv: CsvOptions,
    /// Maximum input size in bytes.
    pub max_bytes: usize,
    /// Maximum total uncompressed size of zip-based formats (xlsx, xlsb, ods).
    pub max_uncompressed_bytes: u64,
    /// Maximum number of rows in the result.
    pub max_rows: usize,
    /// Maximum number of cells (rows × columns) in the result.
    pub max_cells: usize,
}

impl Default for ReadOptions {
    fn default() -> Self {
        ReadOptions {
            format: None,
            sheet: SheetSelector::First,
            csv: CsvOptions::default(),
            max_bytes: 50 * 1024 * 1024,
            max_uncompressed_bytes: 500 * 1024 * 1024,
            max_rows: 1_048_576,
            max_cells: 5_000_000,
        }
    }
}

/// Result of [`read_sheet`]: all sheet names and the rows of the selected sheet.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SheetData {
    pub sheet_names: Vec<String>,
    pub sheet: String,
    pub rows: Vec<Vec<Value>>,
}

/// Reads one worksheet. Rows start at A1; every row has the same length.
/// Formulas yield their last calculated value.
pub fn read_sheet(bytes: &[u8], opts: &ReadOptions) -> Result<SheetData> {
    // Parsers may panic on malformed input. Natively the panic is turned into
    // CORRUPT_FILE; on wasm32 (panic = abort) the JavaScript wrapper handles the trap.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        read_sheet_inner(bytes, opts)
    }))
    .unwrap_or_else(|_| Err(Error::corrupt("malformed file")))
}

fn read_sheet_inner(bytes: &[u8], opts: &ReadOptions) -> Result<SheetData> {
    if bytes.len() > opts.max_bytes {
        return Err(Error::too_large(format!(
            "input exceeds {} bytes",
            opts.max_bytes
        )));
    }
    let format = match opts.format {
        Some(f) => f,
        None => detect(bytes)?,
    };
    if matches!(format, Format::Xlsx | Format::Xlsb | Format::Ods) {
        check_zip_size(bytes, opts.max_uncompressed_bytes)?;
    }
    match format {
        Format::Csv => {
            if !matches!(opts.sheet, SheetSelector::First | SheetSelector::Index(0)) {
                return Err(Error::new(
                    ErrorCode::SheetNotFound,
                    "csv files have a single sheet",
                ));
            }
            let rows = read_csv(bytes, &opts.csv, opts.max_rows, opts.max_cells)?;
            Ok(SheetData {
                sheet_names: vec!["Sheet1".into()],
                sheet: "Sheet1".into(),
                rows,
            })
        }
        Format::Xlsx => read_xlsx(bytes, opts),
        Format::Xlsb => read_xlsb(bytes, opts),
        Format::Xls => from_workbook(Xls::new(Cursor::new(bytes)).map_err(Error::corrupt)?, opts),
        Format::Ods => from_workbook(Ods::new(Cursor::new(bytes)).map_err(Error::corrupt)?, opts),
    }
}

fn from_workbook<RS, R>(mut wb: R, opts: &ReadOptions) -> Result<SheetData>
where
    RS: std::io::Read + std::io::Seek,
    R: Reader<RS>,
    R::Error: std::fmt::Display,
{
    let sheet_names = wb.sheet_names();
    let sheet = select_sheet(&sheet_names, &opts.sheet)?;
    let range = wb.worksheet_range(&sheet).map_err(Error::corrupt)?;
    let rows = range_to_rows(&range, opts)?;
    Ok(SheetData {
        sheet_names,
        sheet,
        rows,
    })
}

/// xlsx and xlsb are read cell by cell, so the limits apply before a dense grid
/// is allocated (a tiny file can reference cell XFD1048576).
fn read_xlsx(bytes: &[u8], opts: &ReadOptions) -> Result<SheetData> {
    let mut wb = Xlsx::new(Cursor::new(bytes)).map_err(Error::corrupt)?;
    let sheet_names = wb.sheet_names();
    let sheet = select_sheet(&sheet_names, &opts.sheet)?;
    let mut reader = wb.worksheet_cells_reader(&sheet).map_err(Error::corrupt)?;
    let mut grid = SparseGrid::new(opts);
    while let Some(cell) = reader.next_cell().map_err(Error::corrupt)? {
        grid.push(cell.get_position(), convert_ref(cell.get_value()))?;
    }
    Ok(SheetData {
        sheet_names,
        sheet,
        rows: grid.into_rows(),
    })
}

fn read_xlsb(bytes: &[u8], opts: &ReadOptions) -> Result<SheetData> {
    let mut wb = Xlsb::new(Cursor::new(bytes)).map_err(Error::corrupt)?;
    let sheet_names = wb.sheet_names();
    let sheet = select_sheet(&sheet_names, &opts.sheet)?;
    let mut reader = wb.worksheet_cells_reader(&sheet).map_err(Error::corrupt)?;
    let mut grid = SparseGrid::new(opts);
    while let Some(cell) = reader.next_cell().map_err(Error::corrupt)? {
        grid.push(cell.get_position(), convert_ref(cell.get_value()))?;
    }
    Ok(SheetData {
        sheet_names,
        sheet,
        rows: grid.into_rows(),
    })
}

/// Collects non-empty cells and enforces the row and cell limits on the
/// dense result size before it is allocated.
struct SparseGrid {
    cells: Vec<(u32, u32, Value)>,
    rows: usize,
    cols: usize,
    max_rows: usize,
    max_cells: usize,
}

impl SparseGrid {
    fn new(opts: &ReadOptions) -> Self {
        SparseGrid {
            cells: Vec::new(),
            rows: 0,
            cols: 0,
            max_rows: opts.max_rows,
            max_cells: opts.max_cells,
        }
    }

    fn push(&mut self, (r, c): (u32, u32), value: Value) -> Result<()> {
        if value == Value::Empty {
            return Ok(());
        }
        self.rows = self.rows.max(r as usize + 1);
        self.cols = self.cols.max(c as usize + 1);
        check_limits(self.rows, self.cols, self.max_rows, self.max_cells)?;
        self.cells.push((r, c, value));
        Ok(())
    }

    fn into_rows(self) -> Vec<Vec<Value>> {
        let mut out = vec![vec![Value::Empty; self.cols]; self.rows];
        for (r, c, v) in self.cells {
            out[r as usize][c as usize] = v;
        }
        out
    }
}

fn check_limits(rows: usize, cols: usize, max_rows: usize, max_cells: usize) -> Result<()> {
    if rows > max_rows {
        return Err(Error::too_large(format!("more than {max_rows} rows")));
    }
    if rows.saturating_mul(cols) > max_cells {
        return Err(Error::too_large(format!("more than {max_cells} cells")));
    }
    Ok(())
}

fn select_sheet(names: &[String], sel: &SheetSelector) -> Result<String> {
    let found = match sel {
        SheetSelector::First => names.first(),
        SheetSelector::Index(i) => names.get(*i),
        SheetSelector::Name(n) => names.iter().find(|s| *s == n),
    };
    found.cloned().ok_or_else(|| {
        let what = match sel {
            SheetSelector::First => "workbook has no sheets".to_string(),
            SheetSelector::Index(i) => format!("no sheet at index {i}"),
            SheetSelector::Name(n) => format!("no sheet named '{n}'"),
        };
        Error::new(ErrorCode::SheetNotFound, what)
    })
}

/// xls and ods: calamine builds the dense range itself, so a far-away cell is
/// allocated before these limits apply. In WebAssembly the memory cap turns that
/// into a trap, which the JavaScript wrapper reports as CORRUPT_FILE.
fn range_to_rows(range: &Range<Data>, opts: &ReadOptions) -> Result<Vec<Vec<Value>>> {
    let Some((end_row, end_col)) = range.end() else {
        return Ok(Vec::new());
    };
    let rows = end_row as usize + 1;
    let cols = end_col as usize + 1;
    if rows > opts.max_rows {
        return Err(Error::too_large(format!(
            "more than {} rows",
            opts.max_rows
        )));
    }
    if rows.saturating_mul(cols) > opts.max_cells {
        return Err(Error::too_large(format!(
            "more than {} cells",
            opts.max_cells
        )));
    }
    let mut out = vec![vec![Value::Empty; cols]; rows];
    let (r0, c0) = range.start().unwrap_or((0, 0));
    for (r, c, data) in range.used_cells() {
        out[r + r0 as usize][c + c0 as usize] = convert(data);
    }
    Ok(out)
}

fn convert(data: &Data) -> Value {
    match data {
        Data::Empty | Data::Error(_) => Value::Empty,
        Data::Int(i) => Value::Number(*i as f64),
        Data::Float(f) => Value::Number(*f),
        Data::String(s) => Value::String(s.clone()),
        Data::Bool(b) => Value::Bool(*b),
        Data::DateTime(dt) if dt.is_duration() => Value::Number(dt.as_f64()),
        Data::DateTime(dt) => Value::Date(excel_datetime_to_ms(dt)),
        Data::DateTimeIso(s) => parse_iso_datetime(s)
            .map(Value::Date)
            .unwrap_or_else(|| Value::String(s.clone())),
        Data::DurationIso(s) => Value::String(s.clone()),
    }
}

fn convert_ref(data: &DataRef<'_>) -> Value {
    match data {
        DataRef::Empty | DataRef::Error(_) => Value::Empty,
        DataRef::Int(i) => Value::Number(*i as f64),
        DataRef::Float(f) => Value::Number(*f),
        DataRef::String(s) => Value::String(s.clone()),
        DataRef::SharedString(s) => Value::String((*s).to_owned()),
        DataRef::Bool(b) => Value::Bool(*b),
        DataRef::DateTime(dt) if dt.is_duration() => Value::Number(dt.as_f64()),
        DataRef::DateTime(dt) => Value::Date(excel_datetime_to_ms(dt)),
        DataRef::DateTimeIso(s) => parse_iso_datetime(s)
            .map(Value::Date)
            .unwrap_or_else(|| Value::String(s.clone())),
        DataRef::DurationIso(s) => Value::String(s.clone()),
    }
}

/// Uses calamine's conversion so the workbook's 1900/1904 date system is respected.
fn excel_datetime_to_ms(dt: &calamine::ExcelDateTime) -> f64 {
    let (y, mo, d, h, mi, s, ms) = dt.to_ymd_hms_milli();
    civil_to_unix_ms(y as i64, mo as u32, d as u32)
        + ((h as f64 * 60.0 + mi as f64) * 60.0 + s as f64) * 1000.0
        + ms as f64
}
