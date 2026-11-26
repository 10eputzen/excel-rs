//! Read and write spreadsheets: xlsx, xls, xlsb, ods and csv in, xlsx out.
//!
//! This crate is the pure-Rust core of the `excel-rs` npm package.

mod csv_read;
mod detect;
mod error;
mod prescan;
mod read;
mod value;
mod write;

pub use csv_read::{CsvEncoding, CsvOptions};
pub use detect::{detect, Format};
pub use error::{Error, ErrorCode, Result};
pub use read::{read_sheet, ReadOptions, SheetData, SheetSelector};
pub use value::Value;
pub use write::write_xlsx_json;
