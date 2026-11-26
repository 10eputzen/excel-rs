use rust_xlsxwriter::{Format, Workbook, Worksheet, XlsxError};
use serde_json::Value as Json;

use crate::error::{Error, ErrorCode, Result};
use crate::value::{is_whole_day, unix_ms_to_serial};

/// Excel's built-in "short date" format; displays in the reader's locale.
const BUILTIN_DATE: u8 = 14;
/// Excel's built-in "short date + time" format; displays in the reader's locale.
const BUILTIN_DATETIME: u8 = 22;
const MAX_ROWS: usize = 1_048_576;
const MAX_COLS: usize = 16_384;
const MAX_WIDTH: u32 = 80;

/// Writes an xlsx workbook from the JSON sheet description used by the
/// JavaScript API (`writeXlsx`). Dates are `{"d": <unix ms>}`.
pub fn write_xlsx_json(json: &str) -> Result<Vec<u8>> {
    let sheets: Json = serde_json::from_str(json).map_err(|e| Error::invalid(e.to_string()))?;
    let sheets = sheets
        .as_array()
        .ok_or_else(|| Error::invalid("expected an array of sheets"))?;
    if sheets.is_empty() {
        return Err(Error::invalid("at least one sheet is required"));
    }
    let mut workbook = Workbook::new();
    for (i, sheet) in sheets.iter().enumerate() {
        let ws = workbook.add_worksheet();
        write_sheet(ws, sheet).map_err(|e| Error {
            code: e.code,
            message: format!("sheet {i}: {}", e.message),
        })?;
    }
    workbook.save_to_buffer().map_err(xlsx_err)
}

struct Column {
    header: String,
    width: Option<f64>,
    format: Option<Format>,
    /// `text` columns: dates still get a date format.
    is_text: bool,
}

fn write_sheet(ws: &mut Worksheet, sheet: &Json) -> Result<()> {
    let obj = sheet
        .as_object()
        .ok_or_else(|| Error::invalid("sheet must be an object"))?;
    let name = obj
        .get("name")
        .and_then(Json::as_str)
        .ok_or_else(|| Error::invalid("sheet.name must be a string"))?;
    ws.set_name(name).map_err(xlsx_err)?;

    let columns = parse_columns(obj.get("columns"))?;
    let rows = match obj.get("rows") {
        None | Some(Json::Null) => &[][..],
        Some(Json::Array(rows)) => rows.as_slice(),
        Some(_) => return Err(Error::invalid("sheet.rows must be an array")),
    };
    let freeze = bool_opt(obj.get("freezeHeader"), true)?;
    let filter = bool_opt(obj.get("autoFilter"), true)?;

    if rows.len() + 1 > MAX_ROWS {
        return Err(Error::too_large(format!("more than {} rows", MAX_ROWS - 1)));
    }

    let bold = Format::new().set_bold();
    let date_fmt = Format::new().set_num_format_index(BUILTIN_DATE);
    let datetime_fmt = Format::new().set_num_format_index(BUILTIN_DATETIME);

    let has_header = !columns.is_empty();
    for (c, col) in columns.iter().enumerate() {
        ws.write_string_with_format(0, c as u16, &col.header, &bold)
            .map_err(xlsx_err)?;
    }
    let first_data_row = u32::from(has_header);

    let mut width = columns.len();
    for (r, row) in rows.iter().enumerate() {
        let cells = row
            .as_array()
            .ok_or_else(|| Error::invalid(format!("row {r} must be an array")))?;
        if cells.len() > MAX_COLS {
            return Err(Error::too_large(format!(
                "row {r} has more than {MAX_COLS} cells"
            )));
        }
        width = width.max(cells.len());
        let xr = first_data_row + r as u32;
        for (c, cell) in cells.iter().enumerate() {
            let xc = c as u16;
            let col = columns.get(c);
            let fmt = col.and_then(|c| c.format.as_ref());
            match cell {
                Json::Null => {}
                Json::Bool(b) => match fmt {
                    Some(f) => ws.write_boolean_with_format(xr, xc, *b, f),
                    None => ws.write_boolean(xr, xc, *b),
                }
                .map(drop)
                .map_err(xlsx_err)?,
                Json::Number(n) => {
                    let n = n.as_f64().unwrap_or(0.0);
                    match fmt {
                        Some(f) => ws.write_number_with_format(xr, xc, n, f),
                        None => ws.write_number(xr, xc, n),
                    }
                    .map_err(xlsx_err)?;
                }
                Json::String(s) => match fmt {
                    Some(f) => ws.write_string_with_format(xr, xc, s, f),
                    None => ws.write_string(xr, xc, s),
                }
                .map(drop)
                .map_err(xlsx_err)?,
                Json::Object(o) => {
                    let ms = o
                        .get("d")
                        .and_then(Json::as_f64)
                        .filter(|v| v.is_finite())
                        .ok_or_else(|| {
                            Error::invalid(format!("row {r}, column {c}: unsupported value"))
                        })?;
                    // Date columns and custom number formats win; otherwise pick
                    // date or date+time depending on the value.
                    let f = match (fmt, col.is_some_and(|c| c.is_text)) {
                        (Some(f), false) => f,
                        _ if is_whole_day(ms) => &date_fmt,
                        _ => &datetime_fmt,
                    };
                    let serial = unix_ms_to_serial(ms);
                    if !(0.0..2_958_466.0).contains(&serial) {
                        return Err(Error::invalid(format!(
                            "row {r}, column {c}: date outside Excel's range (1900–9999)"
                        )));
                    }
                    ws.write_number_with_format(xr, xc, serial, f)
                        .map_err(xlsx_err)?;
                }
                Json::Array(_) => {
                    return Err(Error::invalid(format!(
                        "row {r}, column {c}: unsupported value"
                    )))
                }
            }
        }
    }

    ws.set_autofit_max_width(MAX_WIDTH);
    ws.autofit();
    for (c, col) in columns.iter().enumerate() {
        if let Some(w) = col.width {
            ws.set_column_width(c as u16, w).map_err(xlsx_err)?;
        }
    }
    if has_header && freeze {
        ws.set_freeze_panes(1, 0).map_err(xlsx_err)?;
    }
    if has_header && filter && width > 0 {
        let last_row = first_data_row + rows.len() as u32 - 1;
        ws.autofilter(0, 0, last_row, (width - 1) as u16)
            .map_err(xlsx_err)?;
    }
    Ok(())
}

fn parse_columns(v: Option<&Json>) -> Result<Vec<Column>> {
    let arr = match v {
        None | Some(Json::Null) => return Ok(Vec::new()),
        Some(Json::Array(a)) => a,
        Some(_) => return Err(Error::invalid("sheet.columns must be an array")),
    };
    if arr.len() > MAX_COLS {
        return Err(Error::too_large(format!("more than {MAX_COLS} columns")));
    }
    arr.iter()
        .enumerate()
        .map(|(i, c)| {
            let header = c
                .get("header")
                .and_then(Json::as_str)
                .ok_or_else(|| Error::invalid(format!("columns[{i}].header must be a string")))?
                .to_owned();
            let width = match c.get("width") {
                None | Some(Json::Null) => None,
                Some(w) => Some(
                    w.as_f64()
                        .filter(|w| (0.0..=255.0).contains(w))
                        .ok_or_else(|| {
                            Error::invalid(format!("columns[{i}].width must be 0–255"))
                        })?,
                ),
            };
            let (format, is_text) = match c.get("format") {
                None | Some(Json::Null) => (None, false),
                Some(Json::String(f)) => match f.as_str() {
                    "text" => (Some(Format::new().set_num_format("@")), true),
                    "date" => (
                        Some(Format::new().set_num_format_index(BUILTIN_DATE)),
                        false,
                    ),
                    "datetime" => (
                        Some(Format::new().set_num_format_index(BUILTIN_DATETIME)),
                        false,
                    ),
                    "" => return Err(Error::invalid(format!("columns[{i}].format is empty"))),
                    custom => (Some(Format::new().set_num_format(custom)), false),
                },
                Some(_) => {
                    return Err(Error::invalid(format!(
                        "columns[{i}].format must be a string"
                    )))
                }
            };
            Ok(Column {
                header,
                width,
                format,
                is_text,
            })
        })
        .collect()
}

fn bool_opt(v: Option<&Json>, default: bool) -> Result<bool> {
    match v {
        None | Some(Json::Null) => Ok(default),
        Some(Json::Bool(b)) => Ok(*b),
        Some(_) => Err(Error::invalid("expected a boolean")),
    }
}

fn xlsx_err(e: XlsxError) -> Error {
    let code = match e {
        XlsxError::RowColumnLimitError => ErrorCode::TooLarge,
        _ => ErrorCode::InvalidInput,
    };
    Error::new(code, e.to_string())
}
