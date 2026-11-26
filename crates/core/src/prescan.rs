//! Size checks for xls and ods that run before calamine parses the file.
//!
//! calamine loads every sheet of these formats into a dense grid while the
//! workbook is opened, so a few kilobytes that place a cell far away (xls) or
//! repeat cells (ods) would allocate gigabytes before the regular limits apply.
//! These scans compute an upper bound of that allocation from the raw records.
//! Both fail closed: input they cannot interpret is rejected as corrupt.

use std::io::{BufReader, Cursor};

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
///
/// Mirrors calamine: the globals substream lists each sheet's offset
/// (BoundSheet8); every listed sheet is parsed from its offset, so an offset
/// listed twice is charged twice. Fails closed: whatever cannot be read here is
/// rejected as corrupt.
pub fn check_xls(bytes: &[u8], max_rows: usize, max_cells: usize) -> Result<()> {
    let corrupt = |what: &str| Error::corrupt(format!("xls: {what}"));
    let stream =
        crate::ole::workbook_stream(bytes).map_err(|_| corrupt("no readable workbook stream"))?;

    // Globals: collect the sheet offsets up to the first EOF.
    let mut offsets = Vec::new();
    for record in Records::new(&stream) {
        let (typ, data) = record.map_err(|_| corrupt("truncated record"))?;
        match typ {
            0x0085 if data.len() >= 4 => offsets.push(u32_at(data, 0) as usize),
            0x0085 => return Err(corrupt("short BoundSheet8 record")),
            0x000A => break,
            _ => {}
        }
    }

    let mut budget = Budget::new(max_rows, max_cells);
    for offset in offsets {
        let sheet = stream
            .get(offset..)
            .ok_or_else(|| corrupt("sheet offset outside the stream"))?;
        scan_xls_sheet(sheet, &mut budget).map_err(|e| match e {
            ScanError::TooLarge(e) => e,
            ScanError::Corrupt => corrupt("truncated record"),
        })?;
    }
    Ok(())
}

enum ScanError {
    TooLarge(Error),
    Corrupt,
}

impl From<Error> for ScanError {
    fn from(e: Error) -> Self {
        ScanError::TooLarge(e)
    }
}

/// One worksheet substream, skipping nested substreams (embedded charts) like calamine.
fn scan_xls_sheet(sheet: &[u8], budget: &mut Budget) -> std::result::Result<(), ScanError> {
    let (mut cells, mut formulas) = ((0u64, 0u64), (0u64, 0u64));
    let mut seen_bof = false;
    let mut depth = 0usize;
    for record in Records::new(sheet) {
        let (typ, data) = record.map_err(|_| ScanError::Corrupt)?;
        if depth > 0 {
            match typ {
                0x0809 => depth += 1,
                0x000A => depth -= 1,
                _ => {}
            }
            continue;
        }
        match typ {
            0x0809 if seen_bof => depth += 1,
            0x0809 => seen_bof = true,
            0x000A => break,
            // Dimensions: calamine reserves rows × cols up front
            0x0200 => {
                let (rows, cols) = xls_dimensions(data).ok_or(ScanError::Corrupt)?;
                budget.check(rows, cols)?;
            }
            // Number, Label, RString, BoolErr, Rk, LabelSst
            0x0203 | 0x0204 | 0x00D6 | 0x0205 | 0x027E | 0x00FD if data.len() >= 4 => {
                extend(&mut cells, u16_at(data, 0), u16_at(data, 2));
            }
            // Formula: a cached value and the formula text
            0x0006 if data.len() >= 4 => {
                extend(&mut cells, u16_at(data, 0), u16_at(data, 2));
                extend(&mut formulas, u16_at(data, 0), u16_at(data, 2));
            }
            // MulRk: row, first column, values…, last column
            0x00BD if data.len() >= 6 => {
                extend(&mut cells, u16_at(data, 0), u16_at(data, data.len() - 2));
            }
            _ => continue,
        }
        budget.check(cells.0, cells.1)?;
    }
    budget.add(cells.0, cells.1)?;
    budget.add(formulas.0, formulas.1)?;
    Ok(())
}

/// Rows × cols of a Dimensions record, as calamine reads it (BIFF5: 10 bytes, BIFF8: 14/16).
fn xls_dimensions(d: &[u8]) -> Option<(u64, u64)> {
    let (rf, rl, cf, cl) = match d.len() {
        10 => (
            u64::from(u16_at(d, 0)),
            u64::from(u16_at(d, 2)),
            u64::from(u16_at(d, 4)),
            u64::from(u16_at(d, 6)),
        ),
        14 | 16 => (
            u64::from(u32_at(d, 0)),
            u64::from(u32_at(d, 4)),
            u64::from(u16_at(d, 8)),
            u64::from(u16_at(d, 10)),
        ),
        _ => return None,
    };
    let cf = if cf > 0xFF || cl < cf { 0 } else { cf };
    if rl >= 1 && cl >= 1 {
        // calamine computes `end - start + 1` in u32 with end = last - 1. A start
        // past the end wraps there, so such a record counts as unbounded.
        let span =
            |first: u64, last: u64| (last - 1).checked_sub(first).map_or(u64::MAX, |n| n + 1);
        Some((span(rf, rl), span(cf, cl)))
    } else {
        Some((1, 1))
    }
}

/// BIFF record iterator with calamine's framing: Continue records (0x003C)
/// that follow a record belong to it and are skipped.
struct Records<'a> {
    rest: &'a [u8],
}

impl<'a> Records<'a> {
    fn new(stream: &'a [u8]) -> Self {
        Records { rest: stream }
    }
}

impl<'a> Iterator for Records<'a> {
    type Item = std::result::Result<(u16, &'a [u8]), ()>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.rest.len() < 4 {
            return (!self.rest.is_empty()).then_some(Err(()));
        }
        let typ = u16_at(self.rest, 0);
        let len = u16_at(self.rest, 2) as usize;
        let Some(data) = self.rest.get(4..4 + len) else {
            return Some(Err(()));
        };
        self.rest = &self.rest[4 + len..];
        if self.rest.len() > 4 && u16_at(self.rest, 0) == 0x003C {
            while self.rest.len() > 4 && u16_at(self.rest, 0) == 0x003C {
                let len = u16_at(self.rest, 2) as usize;
                let Some(next) = self.rest.get(4 + len..) else {
                    return Some(Err(()));
                };
                self.rest = next;
            }
        }
        Some(Ok((typ, data)))
    }
}

fn u32_at(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
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
    let corrupt = |what: &str| Error::corrupt(format!("ods: {what}"));
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(Error::corrupt)?;
    let content = zip
        .by_name("content.xml")
        .map_err(|_| corrupt("no content.xml"))?;
    let mut reader = quick_xml::Reader::from_reader(BufReader::new(content));
    // Same reader settings as calamine.
    let config = reader.config_mut();
    config.check_end_names = false;
    config.trim_text(false);
    let mut budget = Budget::new(max_rows, max_cells);
    let mut table = OdsTable::default();
    let mut buf = Vec::new();
    loop {
        let event = reader
            .read_event_into(&mut buf)
            .map_err(|_| corrupt("malformed XML"))?;
        match &event {
            Event::Start(e) | Event::Empty(e) => {
                let empty = matches!(event, Event::Empty(_));
                match e.name().as_ref() {
                    "table:table" => table = OdsTable::default(),
                    "table:table-row" => {
                        table.start_row(repeat(e, "table:number-rows-repeated")?);
                        if empty {
                            table.end_row();
                        }
                    }
                    "table:table-cell" | "table:covered-table-cell" => {
                        let has_value = !empty
                            || raw_attr(e, "office:value-type")?.is_some()
                            || raw_attr(e, "table:formula")?.is_some();
                        table.cell(repeat(e, "table:number-columns-repeated")?, has_value);
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

/// The first attribute with this name, like calamine's `raw_attr` (no
/// duplicate check). Malformed attributes are rejected.
fn raw_attr(e: &BytesStart<'_>, name: &str) -> Result<Option<String>> {
    for attr in e.attributes().with_checks(false) {
        let attr = attr.map_err(|_| Error::corrupt("ods: malformed attribute"))?;
        if attr.key.as_ref() == name {
            let raw: &str = &attr.value;
            let value = quick_xml::escape::unescape(raw)
                .map_err(|_| Error::corrupt("ods: malformed attribute"))?;
            return Ok(Some(value.into_owned()));
        }
    }
    Ok(None)
}

/// A repeat count; a present but unreadable value is rejected, not read as 1.
fn repeat(e: &BytesStart<'_>, name: &str) -> Result<u64> {
    match raw_attr(e, name)? {
        None => Ok(1),
        Some(v) => v
            .trim()
            .parse()
            .map_err(|_| Error::corrupt(format!("ods: invalid {name}"))),
    }
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
