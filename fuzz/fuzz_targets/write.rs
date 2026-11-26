//! Feeds arbitrary JSON to `write_xlsx_json`; whatever it writes must read back.
#![no_main]

use excel_rs_core::{read_sheet, write_xlsx_json, ReadOptions};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(json) = std::str::from_utf8(data) else {
        return;
    };
    if let Ok(bytes) = write_xlsx_json(json) {
        read_sheet(&bytes, &ReadOptions::default()).expect("written file must be readable");
    }
});
