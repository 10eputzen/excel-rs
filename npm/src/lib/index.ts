/**
 * excel-rs for Node.js: read xlsx, xls, xlsb, ods and csv; write xlsx.
 * Synchronous API; the WebAssembly module is loaded on first use.
 */
import { readSheetWith, writeXlsxWith } from './common';
import type { ReadOptions, SheetResult, SheetSpec, WasmApi } from './common';

export { ExcelError } from './common';
export type { Cell, ColumnSpec, ErrorCode, InputFormat, ReadOptions, SheetResult, SheetSpec } from './common';

let api: WasmApi | undefined;
const load = (): WasmApi => (api ??= require('../../wasm/node/excel_rs_wasm.js') as WasmApi);

/**
 * Reads one sheet of a spreadsheet file.
 * Formula cells yield their last calculated value; csv cells are always strings.
 */
export function readSheet(input: Uint8Array | ArrayBuffer, options: ReadOptions = {}): SheetResult {
  return readSheetWith(load(), input, options);
}

/** Writes an xlsx workbook. Strings are written as text (never as formulas). */
export function writeXlsx(sheets: SheetSpec[]): Uint8Array {
  return writeXlsxWith(load(), sheets);
}
