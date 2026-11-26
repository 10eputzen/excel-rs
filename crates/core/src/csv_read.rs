use encoding_rs::{UTF_16BE, UTF_16LE, WINDOWS_1252};

use crate::error::{Error, Result};
use crate::value::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CsvEncoding {
    Auto,
    Utf8,
    Windows1252,
}

#[derive(Debug, Clone, Copy)]
pub struct CsvOptions {
    /// `None` = detect from the first lines.
    pub delimiter: Option<u8>,
    pub encoding: CsvEncoding,
}

impl Default for CsvOptions {
    fn default() -> Self {
        CsvOptions {
            delimiter: None,
            encoding: CsvEncoding::Auto,
        }
    }
}

/// Decodes bytes to text: strips BOMs, handles UTF-16 and falls back to
/// Windows-1252 when the input is not valid UTF-8 (typical for Excel CSV).
pub fn decode(bytes: &[u8], encoding: CsvEncoding) -> Result<String> {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return utf8(rest, encoding);
    }
    if encoding == CsvEncoding::Auto {
        if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
            return Ok(UTF_16LE.decode_without_bom_handling(rest).0.into_owned());
        }
        if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
            return Ok(UTF_16BE.decode_without_bom_handling(rest).0.into_owned());
        }
    }
    utf8(bytes, encoding)
}

fn utf8(bytes: &[u8], encoding: CsvEncoding) -> Result<String> {
    match encoding {
        CsvEncoding::Windows1252 => Ok(WINDOWS_1252
            .decode_without_bom_handling(bytes)
            .0
            .into_owned()),
        CsvEncoding::Utf8 => String::from_utf8(bytes.to_vec())
            .map_err(|e| Error::corrupt(format!("invalid UTF-8: {e}"))),
        CsvEncoding::Auto => match std::str::from_utf8(bytes) {
            Ok(s) => Ok(s.to_owned()),
            Err(_) => Ok(WINDOWS_1252
                .decode_without_bom_handling(bytes)
                .0
                .into_owned()),
        },
    }
}

const CANDIDATES: [u8; 4] = *b";,\t|";

/// Used when the first line has no delimiter: the file has a single column.
/// The ASCII unit separator does not occur in real csv files, so no cell is split.
const SINGLE_COLUMN: u8 = 0x1F;

/// Picks the delimiter that appears most consistently in the first lines.
/// A delimiter must occur in the first line (the header); without one the file
/// has a single column, so `-0,01` in a German one-column file stays one cell.
pub fn detect_delimiter(text: &str) -> u8 {
    let lines: Vec<&str> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .take(20)
        .collect();
    let mut best = (SINGLE_COLUMN, 0usize, false);
    for &d in &CANDIDATES {
        let counts: Vec<usize> = lines.iter().map(|l| count_outside_quotes(l, d)).collect();
        let first = counts.first().copied().unwrap_or(0);
        if first == 0 {
            continue;
        }
        let consistent = counts.iter().all(|&c| c == first);
        let better = match (consistent, best.2) {
            (true, false) => true,
            (false, true) => false,
            _ => first > best.1,
        };
        if better {
            best = (d, first, consistent);
        }
    }
    best.0
}

fn count_outside_quotes(line: &str, d: u8) -> usize {
    let mut in_quotes = false;
    let mut n = 0;
    for &b in line.as_bytes() {
        if b == b'"' {
            in_quotes = !in_quotes;
        } else if b == d && !in_quotes {
            n += 1;
        }
    }
    n
}

/// Parses CSV text into rows of string cells (empty fields become `Empty`).
pub fn read_csv(
    bytes: &[u8],
    opts: &CsvOptions,
    max_rows: usize,
    max_cells: usize,
) -> Result<Vec<Vec<Value>>> {
    let text = decode(bytes, opts.encoding)?;
    let delimiter = opts.delimiter.unwrap_or_else(|| detect_delimiter(&text));
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .delimiter(delimiter)
        .from_reader(text.as_bytes());
    let mut rows: Vec<Vec<Value>> = Vec::new();
    let mut width = 0usize;
    let mut record = csv::StringRecord::new();
    while reader.read_record(&mut record).map_err(Error::corrupt)? {
        // The csv parser skips blank lines; Excel shows them as empty rows.
        // Keep them, so row numbers match what the user sees in Excel.
        let start = record.position().map_or(0, |p| p.byte());
        let blank_lines = skipped_blank_lines(text.as_bytes(), start);
        for _ in 0..blank_lines {
            push_row(&mut rows, Vec::new(), &mut width, max_rows, max_cells)?;
        }
        let cells = record
            .iter()
            .map(|f| {
                if f.is_empty() {
                    Value::Empty
                } else {
                    Value::String(f.to_owned())
                }
            })
            .collect();
        push_row(&mut rows, cells, &mut width, max_rows, max_cells)?;
    }
    // Drop trailing empty lines, then pad to a rectangle.
    while rows
        .last()
        .is_some_and(|r| r.iter().all(|v| *v == Value::Empty))
    {
        rows.pop();
    }
    for row in &mut rows {
        row.resize(width, Value::Empty);
    }
    Ok(rows)
}

/// Number of blank lines the parser skipped before the record that it reports
/// at byte `start`. The reported start can lie before the previous record's
/// line break (after a quoted field) or after it; both are handled.
fn skipped_blank_lines(text: &[u8], start: u64) -> usize {
    let start = usize::try_from(start).unwrap_or(usize::MAX).min(text.len());
    let mut i = start;
    // A start between the `\r` and `\n` of the previous line break.
    if start > 0 && text[start - 1] == b'\r' && text.get(start) == Some(&b'\n') {
        i += 1;
    }
    let mut breaks = 0usize;
    while i < text.len() && (text[i] == b'\n' || text[i] == b'\r') {
        if text[i] == b'\r' && text.get(i + 1) == Some(&b'\n') {
            i += 1;
        }
        i += 1;
        breaks += 1;
    }
    let after_break = start == 0 || matches!(text[start - 1], b'\n' | b'\r');
    if after_break {
        breaks
    } else {
        breaks.saturating_sub(1)
    }
}

fn push_row(
    rows: &mut Vec<Vec<Value>>,
    row: Vec<Value>,
    width: &mut usize,
    max_rows: usize,
    max_cells: usize,
) -> Result<()> {
    if rows.len() >= max_rows {
        return Err(Error::too_large(format!("more than {max_rows} rows")));
    }
    // The result is padded to rows × widest row, so that is what counts.
    *width = (*width).max(row.len());
    if (rows.len() + 1).saturating_mul(*width) > max_cells {
        return Err(Error::too_large(format!("more than {max_cells} cells")));
    }
    rows.push(row);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn german_excel_csv() {
        // "Größe;Preis\nÄpfel;1,50" in Windows-1252
        let bytes: Vec<u8> = b"Gr\xf6\xdfe;Preis\r\n\xc4pfel;\"1,50\"\r\n".to_vec();
        let rows = read_csv(&bytes, &CsvOptions::default(), 100, 100).unwrap();
        assert_eq!(rows[0][0], Value::String("Größe".into()));
        assert_eq!(rows[1][0], Value::String("Äpfel".into()));
        assert_eq!(rows[1][1], Value::String("1,50".into()));
    }

    #[test]
    fn bom_and_comma() {
        let rows = read_csv(
            "\u{feff}a,b\n1,\"x,y\"\n\n".as_bytes(),
            &CsvOptions::default(),
            100,
            100,
        )
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0][0], Value::String("a".into()));
        assert_eq!(rows[1][1], Value::String("x,y".into()));
    }

    #[test]
    fn ragged_rows_are_padded() {
        let rows = read_csv(b"a;b;c\n1\n", &CsvOptions::default(), 100, 100).unwrap();
        assert_eq!(
            rows[1],
            vec![Value::String("1".into()), Value::Empty, Value::Empty]
        );
    }

    #[test]
    fn delimiter_detection() {
        assert_eq!(detect_delimiter("a;b;c\n1;2,5;3\n"), b';');
        assert_eq!(detect_delimiter("a,b\n1,2\n"), b',');
        assert_eq!(detect_delimiter("a\tb\n1\t2\n"), b'\t');
        assert_eq!(detect_delimiter("\"a;b\",c\n1,2\n"), b',');
        // One column: the comma in a German decimal is no delimiter.
        assert_eq!(detect_delimiter("Betrag\n-0,01\n1.234,56\n"), SINGLE_COLUMN);
        let rows = read_csv(
            b"Betrag\r\n-0,01\r\n\"a;b\"\r\n",
            &CsvOptions::default(),
            10,
            100,
        )
        .unwrap();
        assert_eq!(
            rows,
            vec![
                vec![Value::String("Betrag".into())],
                vec![Value::String("-0,01".into())],
                vec![Value::String("a;b".into())],
            ]
        );
    }

    #[test]
    fn cell_limit_counts_padding() {
        // 3 rows, one of them 1000 wide: 3000 cells after padding.
        let mut csv = b"a\nb\n".to_vec();
        csv.extend(std::iter::repeat_n(b';', 999));
        let semi = CsvOptions {
            delimiter: Some(b';'),
            ..Default::default()
        };
        let err = read_csv(&csv, &semi, 100, 2000).unwrap_err();
        assert_eq!(err.code, crate::ErrorCode::TooLarge);
        let wide_first = [vec![b';'; 999], b"\nb\nc\n".to_vec()].concat();
        let err = read_csv(&wide_first, &semi, 100, 2000).unwrap_err();
        assert_eq!(err.code, crate::ErrorCode::TooLarge);
    }

    #[test]
    fn row_limit() {
        let err = read_csv(b"1\n2\n3\n", &CsvOptions::default(), 2, 100).unwrap_err();
        assert_eq!(err.code, crate::ErrorCode::TooLarge);
    }

    #[test]
    fn blank_lines_are_kept_as_empty_rows() {
        let opts = CsvOptions::default();
        let text = "a;b\n1;2\n\n\n3;\"x\ny\"\r\n\r\n4;5\n\n";
        let rows = read_csv(text.as_bytes(), &opts, 100, 1000).unwrap();
        let s = |v: &str| Value::String(v.into());
        assert_eq!(
            rows,
            vec![
                vec![s("a"), s("b")],
                vec![s("1"), s("2")],
                vec![Value::Empty, Value::Empty],
                vec![Value::Empty, Value::Empty],
                vec![s("3"), s("x\ny")],
                vec![Value::Empty, Value::Empty],
                vec![s("4"), s("5")],
            ]
        );
    }
}
