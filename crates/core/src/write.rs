use rust_xlsxwriter::{Color, Format, FormatAlign, Workbook, Worksheet, XlsxError};
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

/// Cell style from the JSON sheet description (`CellStyle` in TypeScript).
#[derive(Clone, Copy, Default, Debug, PartialEq)]
struct Style {
    bold: Option<bool>,
    font_size: Option<f64>,
    font_color: Option<u32>,
    fill: Option<u32>,
    align: Option<FormatAlign>,
}

impl Style {
    fn parse(v: Option<&Json>, what: &str) -> Result<Option<Style>> {
        let obj = match v {
            None | Some(Json::Null) => return Ok(None),
            Some(Json::Object(o)) => o,
            Some(_) => return Err(Error::invalid(format!("{what} must be an object"))),
        };
        let mut st = Style::default();
        match obj.get("bold") {
            None | Some(Json::Null) => {}
            Some(Json::Bool(b)) => st.bold = Some(*b),
            Some(_) => return Err(Error::invalid(format!("{what}.bold must be a boolean"))),
        }
        match obj.get("fontSize") {
            None | Some(Json::Null) => {}
            Some(n) => {
                st.font_size = Some(
                    n.as_f64()
                        .filter(|n| (1.0..=409.0).contains(n))
                        .ok_or_else(|| Error::invalid(format!("{what}.fontSize must be 1–409")))?,
                )
            }
        }
        st.font_color = parse_color(obj.get("fontColor"), &format!("{what}.fontColor"))?;
        st.fill = parse_color(obj.get("fill"), &format!("{what}.fill"))?;
        st.align = match obj.get("align") {
            None | Some(Json::Null) => None,
            Some(Json::String(a)) => Some(match a.as_str() {
                "left" => FormatAlign::Left,
                "center" => FormatAlign::Center,
                "right" => FormatAlign::Right,
                _ => {
                    return Err(Error::invalid(format!(
                        "{what}.align must be left, center or right"
                    )))
                }
            }),
            Some(_) => return Err(Error::invalid(format!("{what}.align must be a string"))),
        };
        Ok(Some(st))
    }

    /// `over` wins property by property.
    fn overlay(base: Option<Style>, over: Option<Style>) -> Option<Style> {
        match (base, over) {
            (None, o) => o,
            (b, None) => b,
            (Some(b), Some(o)) => Some(Style {
                bold: o.bold.or(b.bold),
                font_size: o.font_size.or(b.font_size),
                font_color: o.font_color.or(b.font_color),
                fill: o.fill.or(b.fill),
                align: o.align.or(b.align),
            }),
        }
    }

    fn apply(&self, mut f: Format) -> Format {
        if self.bold == Some(true) {
            f = f.set_bold();
        }
        if let Some(n) = self.font_size {
            f = f.set_font_size(n);
        }
        if let Some(c) = self.font_color {
            f = f.set_font_color(Color::RGB(c));
        }
        if let Some(c) = self.fill {
            f = f.set_background_color(Color::RGB(c));
        }
        if let Some(a) = self.align {
            f = f.set_align(a);
        }
        f
    }
}

/// `#RRGGBB` or `RRGGBB`.
fn parse_color(v: Option<&Json>, what: &str) -> Result<Option<u32>> {
    match v {
        None | Some(Json::Null) => Ok(None),
        Some(Json::String(s)) => {
            let hex = s.strip_prefix('#').unwrap_or(s);
            if hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                Ok(u32::from_str_radix(hex, 16).ok())
            } else {
                Err(Error::invalid(format!("{what} must be #RRGGBB")))
            }
        }
        Some(_) => Err(Error::invalid(format!("{what} must be a string"))),
    }
}

/// Style of cell `c` in data row `r`: row style, overridden by cell style.
fn cell_style(
    row_styles: &[Option<Style>],
    cell_styles: &[Vec<Option<Style>>],
    r: usize,
    c: usize,
) -> Option<Style> {
    let row = row_styles.get(r).copied().flatten();
    let cell = cell_styles
        .get(r)
        .and_then(|row| row.get(c))
        .copied()
        .flatten();
    Style::overlay(row, cell)
}

fn parse_style_list(v: Option<&Json>, what: &str) -> Result<Vec<Option<Style>>> {
    match v {
        None | Some(Json::Null) => Ok(Vec::new()),
        Some(Json::Array(a)) => a
            .iter()
            .enumerate()
            .map(|(i, s)| Style::parse(Some(s), &format!("{what}[{i}]")))
            .collect(),
        Some(_) => Err(Error::invalid(format!("{what} must be an array"))),
    }
}

fn parse_cell_styles(v: Option<&Json>) -> Result<Vec<Vec<Option<Style>>>> {
    match v {
        None | Some(Json::Null) => Ok(Vec::new()),
        Some(Json::Array(rows)) => rows
            .iter()
            .enumerate()
            .map(|(i, row)| parse_style_list(Some(row), &format!("sheet.cellStyles[{i}]")))
            .collect(),
        Some(_) => Err(Error::invalid("sheet.cellStyles must be an array")),
    }
}

struct TitleRow<'a> {
    value: &'a Json,
    style: Option<Style>,
    /// Number of columns the cell spans (1 = no merge).
    span: u16,
}

fn parse_title_rows(v: Option<&Json>) -> Result<Vec<TitleRow<'_>>> {
    let arr = match v {
        None | Some(Json::Null) => return Ok(Vec::new()),
        Some(Json::Array(a)) => a,
        Some(_) => return Err(Error::invalid("sheet.titleRows must be an array")),
    };
    arr.iter()
        .enumerate()
        .map(|(i, t)| {
            let value = t
                .get("value")
                .ok_or_else(|| Error::invalid(format!("titleRows[{i}].value is required")))?;
            let span = match t.get("mergeAcross") {
                None | Some(Json::Null) => 1,
                Some(n) => n
                    .as_u64()
                    .filter(|n| (1..=MAX_COLS as u64).contains(n))
                    .ok_or_else(|| {
                        Error::invalid(format!("titleRows[{i}].mergeAcross must be 1–{MAX_COLS}"))
                    })? as u16,
            };
            Ok(TitleRow {
                value,
                style: Style::parse(t.get("style"), &format!("titleRows[{i}].style"))?,
                span,
            })
        })
        .collect()
}

struct Column {
    header: String,
    width: Option<f64>,
    format: Option<Format>,
    /// `text` columns: dates still get a date format.
    is_text: bool,
}

struct Formats {
    date: Format,
    datetime: Format,
}

/// Writes one cell. `base` is the column format; `style` is added on top.
/// Empty cells with a style are written blank so fills and borders show.
#[allow(clippy::too_many_arguments)]
fn write_cell(
    ws: &mut Worksheet,
    xr: u32,
    xc: u16,
    cell: &Json,
    base: Option<&Format>,
    is_text: bool,
    style: Option<Style>,
    formats: &Formats,
    at: &str,
) -> Result<()> {
    let styled = |b: Option<&Format>| -> Option<Format> {
        style.map(|s| s.apply(b.cloned().unwrap_or_default()))
    };
    match cell {
        Json::Null => {
            if let Some(f) = styled(base) {
                ws.write_blank(xr, xc, &f).map_err(xlsx_err)?;
            }
        }
        Json::Bool(b) => {
            let own = styled(base);
            match own.as_ref().or(base) {
                Some(f) => ws.write_boolean_with_format(xr, xc, *b, f),
                None => ws.write_boolean(xr, xc, *b),
            }
            .map(drop)
            .map_err(xlsx_err)?
        }
        Json::Number(n) => {
            let n = n.as_f64().unwrap_or(0.0);
            let own = styled(base);
            match own.as_ref().or(base) {
                Some(f) => ws.write_number_with_format(xr, xc, n, f),
                None => ws.write_number(xr, xc, n),
            }
            .map_err(xlsx_err)?;
        }
        Json::String(s) => {
            let own = styled(base);
            match own.as_ref().or(base) {
                Some(f) => ws.write_string_with_format(xr, xc, s, f),
                None => ws.write_string(xr, xc, s),
            }
            .map(drop)
            .map_err(xlsx_err)?
        }
        Json::Object(o) => {
            let ms = o
                .get("d")
                .and_then(Json::as_f64)
                .filter(|v| v.is_finite())
                .ok_or_else(|| Error::invalid(format!("{at}: unsupported value")))?;
            // Date columns and custom number formats win; otherwise pick
            // date or date+time depending on the value.
            let f = match (base, is_text) {
                (Some(f), false) => f,
                _ if is_whole_day(ms) => &formats.date,
                _ => &formats.datetime,
            };
            let own = styled(Some(f));
            let serial = unix_ms_to_serial(ms);
            if !(0.0..2_958_466.0).contains(&serial) {
                return Err(Error::invalid(format!(
                    "{at}: date outside Excel's range (1900–9999)"
                )));
            }
            ws.write_number_with_format(xr, xc, serial, own.as_ref().unwrap_or(f))
                .map_err(xlsx_err)?;
        }
        Json::Array(_) => return Err(Error::invalid(format!("{at}: unsupported value"))),
    }
    Ok(())
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
    let protect = bool_opt(obj.get("protect"), false)?;
    let header_style = Style::parse(obj.get("headerStyle"), "sheet.headerStyle")?;
    let title_rows = parse_title_rows(obj.get("titleRows"))?;
    let row_styles = parse_style_list(obj.get("rowStyles"), "sheet.rowStyles")?;
    let cell_styles = parse_cell_styles(obj.get("cellStyles"))?;

    if title_rows.len() + rows.len() + 1 > MAX_ROWS {
        return Err(Error::too_large(format!("more than {} rows", MAX_ROWS - 1)));
    }

    let formats = Formats {
        date: Format::new().set_num_format_index(BUILTIN_DATE),
        datetime: Format::new().set_num_format_index(BUILTIN_DATETIME),
    };
    let header_fmt = match header_style {
        Some(s) => s.apply(Format::new()),
        None => Format::new().set_bold(),
    };

    let has_header = !columns.is_empty();
    let header_row = title_rows.len() as u32;
    for (c, col) in columns.iter().enumerate() {
        ws.write_string_with_format(header_row, c as u16, &col.header, &header_fmt)
            .map_err(xlsx_err)?;
    }
    let first_data_row = header_row + u32::from(has_header);

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
            let col = columns.get(c);
            write_cell(
                ws,
                xr,
                c as u16,
                cell,
                col.and_then(|c| c.format.as_ref()),
                col.is_some_and(|c| c.is_text),
                cell_style(&row_styles, &cell_styles, r, c),
                &formats,
                &format!("row {r}, column {c}"),
            )?;
        }
        // A row style also colors the cells the row does not fill.
        if row_styles.get(r).copied().flatten().is_some() {
            for c in cells.len()..columns.len() {
                write_cell(
                    ws,
                    xr,
                    c as u16,
                    &Json::Null,
                    None,
                    false,
                    cell_style(&row_styles, &cell_styles, r, c),
                    &formats,
                    "",
                )?;
            }
        }
    }

    // Autofit before the title rows: a long title must not widen column A.
    ws.set_autofit_max_width(MAX_WIDTH);
    ws.autofit();
    for (c, col) in columns.iter().enumerate() {
        if let Some(w) = col.width {
            ws.set_column_width(c as u16, w).map_err(xlsx_err)?;
        }
    }
    for (i, t) in title_rows.iter().enumerate() {
        let r = i as u32;
        let at = format!("titleRows[{i}]");
        let own = t.style.map(|s| s.apply(Format::new()));
        if t.span > 1 {
            let fmt = own.clone().unwrap_or_default();
            ws.merge_range(r, 0, r, t.span - 1, "", &fmt)
                .map_err(xlsx_err)?;
        }
        if t.value.is_null() && t.span <= 1 && own.is_none() {
            continue;
        }
        write_cell(ws, r, 0, t.value, None, false, t.style, &formats, &at)?;
    }
    if has_header && freeze {
        ws.set_freeze_panes(header_row + 1, 0).map_err(xlsx_err)?;
    }
    if has_header && filter && width > 0 {
        let last_row = first_data_row + rows.len() as u32 - 1;
        ws.autofilter(header_row, 0, last_row, (width - 1) as u16)
            .map_err(xlsx_err)?;
    }
    if protect {
        ws.protect();
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
