/**
 * excel-rs for Node.js: read xlsx, xls, xlsb, ods and csv; write xlsx.
 * Synchronous API; the WebAssembly module is loaded on first use.
 */
import { ExcelError, readSheetWith, writeXlsxWith } from './common';
import type { ReadOptions, SheetResult, SheetSpec, WasmApi } from './common';
import { applySchema, validateSchema } from './schema';
import type { Schema, SchemaResult } from './schema';

export { ExcelError } from './common';
export type { Cell, ColumnSpec, ErrorCode, InputFormat, ReadOptions, SheetResult, SheetSpec } from './common';
export type {
  AnyColumnRule,
  ColumnRule,
  ColumnType,
  Schema,
  SchemaError,
  SchemaErrorCode,
  SchemaResult,
  SchemaRow,
} from './schema';

/** Options of `readSheet` with a schema. */
export interface SchemaReadOptions<S extends Schema> extends ReadOptions {
  /** Columns to read, found by their header in the first row. */
  schema: S;
  /** Errors to collect at most. Default 1,000. */
  maxErrors?: number;
}

let api: WasmApi | undefined;
const load = (): WasmApi => (api ??= require('../../wasm/node/excel_rs_wasm.js') as WasmApi);

/**
 * Reads one sheet of a spreadsheet file.
 * Formula cells yield their last calculated value; csv cells are always strings.
 *
 * With `schema`, the first row is the header and every further row becomes an
 * object with converted values; cells that break a rule end up in `errors`
 * with a German message instead.
 */
export function readSheet<S extends Schema>(input: Uint8Array | ArrayBuffer, options: SchemaReadOptions<S>): SchemaResult<S>;
export function readSheet(input: Uint8Array | ArrayBuffer, options?: ReadOptions): SheetResult;
export function readSheet(
  input: Uint8Array | ArrayBuffer,
  options: ReadOptions | SchemaReadOptions<Schema> = {},
): SheetResult | SchemaResult<Schema> {
  if (options === null || typeof options !== 'object' || !('schema' in options)) {
    return readSheetWith(load(), input, options);
  }
  const { schema, maxErrors = 1000, ...readOptions } = options;
  validateSchema(schema);
  if (!Number.isSafeInteger(maxErrors) || maxErrors < 0) {
    throw new ExcelError('INVALID_INPUT', 'maxErrors must be a non-negative integer');
  }
  return applySchema(readSheetWith(load(), input, readOptions).rows, schema, maxErrors);
}

/** Writes an xlsx workbook. Strings are written as text (never as formulas). */
export function writeXlsx(sheets: SheetSpec[]): Uint8Array {
  return writeXlsxWith(load(), sheets);
}
