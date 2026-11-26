/**
 * excel-rs: read xlsx, xls, xlsb, ods and csv; write xlsx.
 * The parsing and writing runs in a Rust core compiled to WebAssembly.
 */

/** A cell value. Dates are `Date` objects in UTC (Excel has no time zones). */
export type Cell = string | number | boolean | Date | null;

export type InputFormat = 'auto' | 'xlsx' | 'xls' | 'xlsb' | 'ods' | 'csv';

export type ErrorCode =
  | 'UNSUPPORTED_FORMAT'
  | 'CORRUPT_FILE'
  | 'SHEET_NOT_FOUND'
  | 'TOO_LARGE'
  | 'INVALID_INPUT';

export interface ReadOptions {
  /** Input format. `auto` (default) detects it from the file content. */
  format?: InputFormat;
  /** Sheet name or zero-based index. Default: the first sheet. */
  sheet?: string | number;
  csv?: {
    /** Field delimiter. `auto` (default) detects `;`, `,`, tab or `|`. */
    delimiter?: ',' | ';' | '\t' | '|' | 'auto';
    /** Text encoding. `auto` (default): UTF-8/UTF-16 with BOM, else UTF-8, else Windows-1252. */
    encoding?: 'auto' | 'utf-8' | 'windows-1252';
  };
  /** Maximum input size in bytes. Default 50 MiB. */
  maxBytes?: number;
  /** Maximum uncompressed size of xlsx/xlsb/ods archives. Default 500 MiB. */
  maxUncompressedBytes?: number;
  /** Maximum number of rows. Default 1,048,576. */
  maxRows?: number;
  /** Maximum number of cells (rows × columns) of the result. Default 5,000,000. */
  maxCells?: number;
}

export interface SheetResult {
  /** Names of all sheets in the workbook (`['Sheet1']` for csv). */
  sheetNames: string[];
  /** Name of the sheet that was read. */
  sheet: string;
  /** Rows starting at A1. All rows have the same length; empty cells are `null`. */
  rows: Cell[][];
}

export interface ColumnSpec {
  header: string;
  /** Column width in characters. Default: fitted to the content (max 80). */
  width?: number;
  /**
   * `text`, `date` (locale short date), `datetime` (locale date + time)
   * or an Excel number format such as `'#,##0.00'`.
   */
  format?: 'text' | 'date' | 'datetime' | (string & {});
}

export interface SheetSpec {
  name: string;
  /** Header row. Leave empty to write `rows` without a header. */
  columns: ColumnSpec[];
  rows: Cell[][];
  /** Freeze the header row. Default true. */
  freezeHeader?: boolean;
  /** Add an auto filter to the header row. Default true. */
  autoFilter?: boolean;
}

/** Error thrown by `readSheet` and `writeXlsx`. */
export class ExcelError extends Error {
  readonly code: ErrorCode;

  constructor(code: ErrorCode, message: string) {
    super(message);
    this.name = 'ExcelError';
    this.code = code;
  }
}

interface WasmApi {
  readSheet(bytes: Uint8Array, opts: string): string;
  writeXlsx(sheets: string): Uint8Array;
  memoryBytes(): number;
}

const DEFAULT_MAX_BYTES = 50 * 1024 * 1024;
/** WebAssembly memory never shrinks; above this size the instance is dropped after a call. */
const MEMORY_RESET_BYTES = 256 * 1024 * 1024;

// `WebAssembly` types are not part of the Node-only lib; the class is looked up at runtime.
const RuntimeError = (globalThis as unknown as { WebAssembly: { RuntimeError: unknown } }).WebAssembly
  .RuntimeError;
const WASM_PATH = require.resolve('../wasm/excel_rs_wasm.js');
let wasm: WasmApi | undefined;

function load(): WasmApi {
  if (!wasm) wasm = require(WASM_PATH) as WasmApi;
  return wasm;
}

function unload(): void {
  delete require.cache[WASM_PATH];
  wasm = undefined;
}

/**
 * Runs a WASM call. A trap leaves the instance unusable, and a large call leaves
 * its memory allocated; in both cases a fresh instance is loaded on the next call.
 */
function call<T>(fn: (api: WasmApi) => T, onTrap: ErrorCode): T {
  const api = load();
  try {
    return fn(api);
  } catch (err) {
    if (typeof err === 'string') throw toExcelError(err);
    if (err instanceof Error && err.constructor === RuntimeError) {
      unload();
      throw new ExcelError(onTrap, `internal error: ${err.message}`);
    }
    throw err;
  } finally {
    if (wasm === api && api.memoryBytes() > MEMORY_RESET_BYTES) unload();
  }
}

function toExcelError(raw: string): ExcelError {
  try {
    const { code, message } = JSON.parse(raw) as { code: ErrorCode; message: string };
    return new ExcelError(code, message);
  } catch {
    return new ExcelError('INVALID_INPUT', raw);
  }
}

function toBytes(input: Uint8Array | ArrayBuffer): Uint8Array {
  if (input instanceof Uint8Array) return input;
  if (input instanceof ArrayBuffer) return new Uint8Array(input);
  throw new ExcelError('INVALID_INPUT', 'input must be a Uint8Array, Buffer or ArrayBuffer');
}

/**
 * Reads one sheet of a spreadsheet file.
 * Formula cells yield their last calculated value; csv cells are always strings.
 */
export function readSheet(input: Uint8Array | ArrayBuffer, options: ReadOptions = {}): SheetResult {
  validateOptions(options);
  const bytes = toBytes(input);
  // Checked here too, so oversized input is not copied into WebAssembly memory.
  const maxBytes = options.maxBytes ?? DEFAULT_MAX_BYTES;
  if (bytes.byteLength > maxBytes) {
    throw new ExcelError('TOO_LARGE', `input exceeds ${maxBytes} bytes`);
  }
  const json = call((api) => api.readSheet(bytes, JSON.stringify(options)), 'CORRUPT_FILE');
  const result = JSON.parse(json) as { sheetNames: string[]; sheet: string; rows: unknown[][] };
  for (const row of result.rows) {
    for (let i = 0; i < row.length; i++) {
      const v = row[i];
      if (v !== null && typeof v === 'object') row[i] = new Date((v as { d: number }).d);
    }
  }
  return result as SheetResult;
}

function validateOptions(o: ReadOptions): void {
  if (o === null || typeof o !== 'object') throw new ExcelError('INVALID_INPUT', 'options must be an object');
  const isCount = (v: unknown) => typeof v === 'number' && Number.isSafeInteger(v) && v >= 0;
  if (o.sheet !== undefined && typeof o.sheet !== 'string' && !isCount(o.sheet)) {
    throw new ExcelError('INVALID_INPUT', 'sheet must be a name or a non-negative integer index');
  }
  for (const key of ['maxBytes', 'maxUncompressedBytes', 'maxRows', 'maxCells'] as const) {
    if (o[key] !== undefined && !isCount(o[key])) {
      throw new ExcelError('INVALID_INPUT', `${key} must be a non-negative integer`);
    }
  }
}

/** Writes an xlsx workbook. Strings are written as text (never as formulas). */
export function writeXlsx(sheets: SheetSpec[]): Uint8Array {
  if (!Array.isArray(sheets)) throw new ExcelError('INVALID_INPUT', 'expected an array of sheets');
  const payload = sheets.map((s) => ({
    ...s,
    rows: Array.isArray(s.rows) ? s.rows.map((row) => (Array.isArray(row) ? row.map(encodeCell) : row)) : s.rows,
  }));
  return call((api) => api.writeXlsx(JSON.stringify(payload)), 'INVALID_INPUT');
}

function encodeCell(v: unknown): unknown {
  if (v instanceof Date) {
    const t = v.getTime();
    return Number.isNaN(t) ? null : { d: t };
  }
  if (v === undefined) return null;
  if (typeof v === 'number' && !Number.isFinite(v)) return null;
  if (typeof v === 'bigint') return Number(v);
  return v;
}
