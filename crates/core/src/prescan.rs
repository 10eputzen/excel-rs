//! Size checks for xls and ods that run before calamine parses the file.
//!
//! calamine loads every sheet of these formats into a dense grid while the
//! workbook is opened, so a few kilobytes that place a cell far away (xls) or
//! repeat cells (ods) would allocate gigabytes before the regular limits apply.
//! These scans compute an upper bound of that allocation from the raw records.
//! On input they cannot interpret they do nothing and leave the error to calamine.

use std::io::{BufReader, Cursor, Read};

use quick_xml::events::{BytesStart, Event};

use crate::error::{Error, Result};

/// Largest sheet calamine creates (Excel limits).
const MAX_ROWS: u64 = 1_048_576;
const MAX_COLS: u64 = 16_384;

/// Running total of the cells calamine would allocate.
struct Budget {
    cells: u64,
    max_rows: u64,
    max_cells: u64,
}

impl Budget {
    fn new(max_rows: usize, max_cells: usize) -> Self {
        Budget {
            cells: 0,
            max_rows: max_rows as u64,
            max_cells: max_cells as u64,
        }
    }

    /// Checks a sheet extent (rows × cols) on top of the sheets already counted.
    fn check(&self, rows: u64, cols: u64) -> Result<()> {
        if rows > self.max_rows {
            return Err(Error::too_large(format!(
                "more than {} rows",
                self.max_rows
            )));
        }
        if self.cells.saturating_add(rows.saturating_mul(cols)) > self.max_cells {
            return Err(Error::too_large(format!(
                "more than {} cells",
                self.max_cells
            )));
        }
        Ok(())
    }

    fn add(&mut self, rows: u64, cols: u64) -> Result<()> {
        self.check(rows, cols)?;
        self.cells += rows * cols;
        Ok(())
    }
}

/// Bounds the dense grids of all sheets of an xls workbook (BIFF5/8 records).
pub fn check_xls(bytes: &[u8], max_rows: usize, max_cells: usize) -> Result<()> {
    let Ok(mut cfb) = cfb::CompoundFile::open(Cursor::new(bytes)) else {
        return Ok(());
    };
    let mut stream = Vec::new();
    let opened = cfb
        .open_stream("/Workbook")
        .or_else(|_| cfb.open_stream("/Book"));
    let Ok(read) = opened.and_then(|mut s| s.read_to_end(&mut stream)) else {
        return Ok(());
    };
    let _ = read;

    let mut budget = Budget::new(max_rows, max_cells);
    let (mut cells, mut formulas) = ((0u64, 0u64), (0u64, 0u64));
    let mut pos = 0usize;
    while pos + 4 <= stream.len() {
        let typ = u16_at(&stream, pos);
        let len = u16_at(&stream, pos + 2) as usize;
        let Some(data) = stream.get(pos + 4..pos + 4 + len) else {
            break;
        };
        pos += 4 + len;
        match typ {
            // BOF: a new substream (sheet) starts
            0x0809 => {
                budget.add(cells.0, cells.1)?;
                budget.add(formulas.0, formulas.1)?;
                cells = (0, 0);
                formulas = (0, 0);
            }
            // Number, Label, RString, BoolErr, Rk, LabelSst
            0x0203 | 0x0204 | 0x00D6 | 0x0205 | 0x027E | 0x00FD if len >= 4 => {
                extend(&mut cells, u16_at(data, 0), u16_at(data, 2));
            }
            // Formula: a cached value and the formula text
            0x0006 if len >= 4 => {
                extend(&mut cells, u16_at(data, 0), u16_at(data, 2));
                extend(&mut formulas, u16_at(data, 0), u16_at(data, 2));
            }
            // MulRk: row, first column, values…, last column
            0x00BD if len >= 6 => {
                extend(&mut cells, u16_at(data, 0), u16_at(data, len - 2));
            }
            _ => continue,
        }
        budget.check(cells.0, cells.1)?;
    }
    budget.add(cells.0, cells.1)?;
    budget.add(formulas.0, formulas.1)
}

fn u16_at(b: &[u8], i: usize) -> u16 {
    u16::from_le_bytes([b[i], b[i + 1]])
}

fn extend(extent: &mut (u64, u64), row: u16, col: u16) {
    extent.0 = extent.0.max(u64::from(row) + 1);
    extent.1 = extent.1.max(u64::from(col) + 1);
}

/// Bounds the dense grids of all tables of an ods workbook, expanding the
/// `number-rows-repeated` and `number-columns-repeated` attributes the way
/// calamine does.
pub fn check_ods(bytes: &[u8], max_rows: usize, max_cells: usize) -> Result<()> {
    let Ok(mut zip) = zip::ZipArchive::new(Cursor::new(bytes)) else {
        return Ok(());
    };
    let Ok(content) = zip.by_name("content.xml") else {
        return Ok(());
    };
    let mut reader = quick_xml::Reader::from_reader(BufReader::new(content));
    let mut budget = Budget::new(max_rows, max_cells);
    let mut table = OdsTable::default();
    let mut buf = Vec::new();
    loop {
        let event = match reader.read_event_into(&mut buf) {
            Ok(e) => e,
            Err(_) => return Ok(()),
        };
        match &event {
            Event::Start(e) | Event::Empty(e) => {
                let empty = matches!(event, Event::Empty(_));
                match e.name().as_ref() {
                    "table:table" => table = OdsTable::default(),
                    "table:table-row" => {
                        table.start_row(repeat(e, "table:number-rows-repeated"));
                        if empty {
                            table.end_row();
                        }
                    }
                    "table:table-cell" | "table:covered-table-cell" => {
                        let has_value = !empty
                            || e.try_get_attribute("office:value-type")
                                .ok()
                                .flatten()
                                .is_some()
                            || e.try_get_attribute("table:formula")
                                .ok()
                                .flatten()
                                .is_some();
                        table.cell(repeat(e, "table:number-columns-repeated"), has_value);
                    }
                    _ => {}
                }
            }
            Event::End(e) => match e.name().as_ref() {
                "table:table-row" => {
                    table.end_row();
                    budget.check(table.rows, table.cols)?;
                    // calamine expands every value cell, even if the extent is small
                    budget.check(1, table.value_cells)?;
                }
                // Values and formulas each get a grid
                "table:table" => {
                    budget.add(table.rows, table.cols)?;
                    budget.add(table.rows, table.cols)?;
                }
                _ => {}
            },
            Event::Eof => return Ok(()),
            _ => {}
        }
        buf.clear();
    }
}

fn repeat(e: &BytesStart<'_>, name: &str) -> u64 {
    e.try_get_attribute(name)
        .ok()
        .flatten()
        .and_then(|a| a.value.parse().ok())
        .unwrap_or(1)
}

/// Extent of the cells with a value in one ods table.
#[derive(Default)]
struct OdsTable {
    /// Rows before the current one (after repeats).
    row: u64,
    row_repeat: u64,
    col: u64,
    row_last_col: u64,
    row_value_cells: u64,
    rows: u64,
    cols: u64,
    value_cells: u64,
}

impl OdsTable {
    fn start_row(&mut self, repeat: u64) {
        self.row_repeat = repeat.min(MAX_ROWS);
        self.col = 0;
        self.row_last_col = 0;
        self.row_value_cells = 0;
    }

    fn cell(&mut self, repeat: u64, has_value: bool) {
        let repeat = repeat.min(MAX_COLS.saturating_sub(self.col));
        self.col += repeat;
        if has_value {
            self.row_last_col = self.col;
            self.row_value_cells += repeat;
        }
    }

    fn end_row(&mut self) {
        let end = (self.row + self.row_repeat).min(MAX_ROWS);
        if self.row_value_cells > 0 {
            self.rows = end;
            self.cols = self.cols.max(self.row_last_col);
            self.value_cells = self
                .value_cells
                .saturating_add(self.row_value_cells.saturating_mul(self.row_repeat));
        }
        self.row = end;
    }
}
