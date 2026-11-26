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

/// Picks the delimiter that appears most consistently in the first lines.
pub fn detect_delimiter(text: &str) -> u8 {
    let lines: Vec<&str> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .take(20)
        .collect();
    let mut best = (b',', 0usize, false);
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
    let mut cells = 0usize;
    for record in reader.records() {
        let record = record.map_err(Error::corrupt)?;
        if rows.len() >= max_rows {
            return Err(Error::too_large(format!("more than {max_rows} rows")));
        }
        cells += record.len();
        if cells > max_cells {
            return Err(Error::too_large(format!("more than {max_cells} cells")));
        }
        width = width.max(record.len());
        rows.push(
            record
                .iter()
                .map(|f| {
                    if f.is_empty() {
                        Value::Empty
                    } else {
                        Value::String(f.to_owned())
                    }
                })
                .collect(),
        );
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
    }

    #[test]
    fn row_limit() {
        let err = read_csv(b"1\n2\n3\n", &CsvOptions::default(), 2, 100).unwrap_err();
        assert_eq!(err.code, crate::ErrorCode::TooLarge);
    }
}
