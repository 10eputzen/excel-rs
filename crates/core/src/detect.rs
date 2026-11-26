use std::io::Cursor;

use crate::error::{Error, ErrorCode, Result};

/// Supported input formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Xlsx,
    Xls,
    Xlsb,
    Ods,
    Csv,
}

impl Format {
    pub fn parse(s: &str) -> Result<Option<Format>> {
        Ok(Some(match s {
            "auto" => return Ok(None),
            "xlsx" | "xlsm" => Format::Xlsx,
            "xls" => Format::Xls,
            "xlsb" => Format::Xlsb,
            "ods" => Format::Ods,
            "csv" => Format::Csv,
            other => {
                return Err(Error::new(
                    ErrorCode::UnsupportedFormat,
                    format!("unknown format '{other}'"),
                ))
            }
        }))
    }
}

const ZIP_MAGIC: &[u8] = b"PK\x03\x04";
const OLE_MAGIC: &[u8] = &[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

/// Detects the format from the file content (magic bytes and zip entries).
pub fn detect(bytes: &[u8]) -> Result<Format> {
    if bytes.starts_with(OLE_MAGIC) {
        return Ok(Format::Xls);
    }
    if bytes.starts_with(ZIP_MAGIC) {
        let archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(Error::corrupt)?;
        let has = |name: &str| archive.index_for_name(name).is_some();
        return if has("xl/workbook.xml") {
            Ok(Format::Xlsx)
        } else if has("xl/workbook.bin") {
            Ok(Format::Xlsb)
        } else if has("content.xml") {
            Ok(Format::Ods)
        } else {
            Err(Error::new(
                ErrorCode::UnsupportedFormat,
                "zip archive is not a spreadsheet",
            ))
        };
    }
    if looks_like_text(bytes) {
        return Ok(Format::Csv);
    }
    Err(Error::new(
        ErrorCode::UnsupportedFormat,
        "unrecognised file content",
    ))
}

fn looks_like_text(bytes: &[u8]) -> bool {
    let sample = &bytes[..bytes.len().min(4096)];
    // UTF-16 text contains NUL bytes but starts with a BOM.
    if sample.starts_with(&[0xFF, 0xFE]) || sample.starts_with(&[0xFE, 0xFF]) {
        return true;
    }
    !sample.contains(&0)
}

/// Rejects zip archives whose declared uncompressed size exceeds the limit.
pub fn check_zip_size(bytes: &[u8], max_uncompressed: u64) -> Result<()> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(Error::corrupt)?;
    let mut total: u64 = 0;
    for i in 0..archive.len() {
        let entry = archive.by_index_raw(i).map_err(Error::corrupt)?;
        total = total.saturating_add(entry.size());
        if total > max_uncompressed {
            return Err(Error::too_large(format!(
                "uncompressed size exceeds {max_uncompressed} bytes"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_text_and_binary() {
        assert_eq!(detect(b"a;b\n1;2\n").unwrap(), Format::Csv);
        assert_eq!(detect(OLE_MAGIC).unwrap(), Format::Xls);
        assert_eq!(
            detect(&[0u8, 1, 2, 3]).unwrap_err().code,
            ErrorCode::UnsupportedFormat
        );
        assert_eq!(
            detect(b"PK\x03\x04garbage").unwrap_err().code,
            ErrorCode::CorruptFile
        );
    }
}
