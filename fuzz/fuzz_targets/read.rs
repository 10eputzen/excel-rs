//! Feeds arbitrary bytes to `read_sheet`, with format detection or a forced
//! format (first byte). Findings: hangs, excessive memory, and panics.
//!
//! `read_sheet` turns panics into CORRUPT_FILE natively, and the JavaScript
//! wrapper does the same for a WebAssembly trap, so a panic inside a parser
//! dependency is handled. It is logged and the run goes on; collect such
//! inputs into `fixtures/broken/`. A panic in this crate's own code aborts.
#![no_main]

use excel_rs_core::{read_sheet, Format, ReadOptions};
use libfuzzer_sys::fuzz_target;

fn opts(format: Option<Format>) -> ReadOptions {
    // Small limits keep each run fast; the limit logic itself is under test.
    ReadOptions {
        format,
        max_bytes: 4 * 1024 * 1024,
        max_uncompressed_bytes: 64 * 1024 * 1024,
        max_rows: 100_000,
        max_cells: 1_000_000,
        ..Default::default()
    }
}

fuzz_target!(
    init: {
        // Abort before `catch_unwind` in `read_sheet` can swallow the panic.
        std::panic::set_hook(Box::new(|info| {
            let in_dependency = info
                .location()
                .is_some_and(|l| l.file().contains(".cargo/registry") || l.file().contains(".cargo\\registry"));
            if !in_dependency || std::env::var_os("FUZZ_ABORT_ON_ANY_PANIC").is_some() {
                eprintln!("{info}");
                std::process::abort();
            }
        }));
    },
    |data: &[u8]| {
        let Some((&selector, bytes)) = data.split_first() else { return };
        let format = match selector % 6 {
            0 => None,
            1 => Some(Format::Xlsx),
            2 => Some(Format::Xls),
            3 => Some(Format::Xlsb),
            4 => Some(Format::Ods),
            _ => Some(Format::Csv),
        };
        let _ = read_sheet(bytes, &opts(format));
    }
);
