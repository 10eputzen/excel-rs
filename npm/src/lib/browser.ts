/**
 * excel-rs for browsers (and any runtime with ES modules and fetch):
 * read xlsx, xls, xlsb, ods and csv; write xlsx.
 *
 * Load it lazily where it is needed, e.g. `await import('@10eputzen/excel-rs/browser')`.
 * The WebAssembly file (~0.7 MB gzip) is fetched on the first call or by `initExcel()`.
 *
 * @deprecated Since 0.2.0. Read files on the server with the Node.js entry and
 * check them before the upload with `excel-rs/detect`. Removed in a later major version.
 */
import init, * as bindings from '../../wasm/web/excel_rs_wasm.js';
import { ExcelError, readSheetWith, writeXlsxWith } from './common.js';
import type { ReadOptions, SheetResult, SheetSpec, WasmApi } from './common.js';

export { ExcelError } from './common.js';
export type { Cell, ColumnSpec, ErrorCode, InputFormat, ReadOptions, SheetResult, SheetSpec } from './common.js';

/**
 * Where to load the WebAssembly binary from: a URL (string, `URL`, `Request`),
 * a `Response`, the bytes, or a compiled `WebAssembly.Module`.
 * Default: `excel_rs_wasm_bg.wasm` next to the bindings, resolved via `import.meta.url`.
 */
export type WasmSource = Parameters<typeof init>[0] extends infer P
  ? P extends { module_or_path?: infer S } ? S : never
  : never;

let ready: Promise<WasmApi> | undefined;
let readySource: WasmSource | undefined;

/**
 * Loads the WebAssembly module. Optional: the first `readSheet`/`writeXlsx`
 * call does it too. Call it early with your own URL when the bundler does not
 * copy the `.wasm` file (see README). Rejects with `WASM_LOAD_FAILED` when the
 * file cannot be fetched or compiled; a later call retries.
 * @deprecated Since 0.2.0. Read files on the server with the Node.js entry and
 * check them before the upload with `excel-rs/detect`. Removed in a later major version.
 */
export function initExcel(source?: WasmSource): Promise<void> {
  if (ready && source !== undefined && source !== readySource) {
    return Promise.reject(
      new ExcelError('INVALID_INPUT', 'excel-rs is already loaded or loading from another source'),
    );
  }
  if (!ready) {
    readySource = source;
    ready = init(source === undefined ? undefined : { module_or_path: source }).then(
      () => bindings as unknown as WasmApi,
      (err: unknown) => {
        ready = undefined; // allow a retry, e.g. after a network error
        readySource = undefined;
        const reason = err instanceof Error ? err.message : String(err);
        throw new ExcelError('WASM_LOAD_FAILED', `could not load excel-rs WebAssembly: ${reason}`);
      },
    );
  }
  return ready.then(() => undefined);
}

async function load(): Promise<WasmApi> {
  await initExcel();
  return ready as Promise<WasmApi>;
}

/**
 * Reads one sheet of a spreadsheet file.
 * Formula cells yield their last calculated value; csv cells are always strings.
 * @deprecated Since 0.2.0. Read files on the server with the Node.js entry and
 * check them before the upload with `excel-rs/detect`. Removed in a later major version.
 */
export async function readSheet(input: Uint8Array | ArrayBuffer, options: ReadOptions = {}): Promise<SheetResult> {
  return readSheetWith(await load(), input, options);
}

/**
 * Writes an xlsx workbook. Strings are written as text (never as formulas).
 * @deprecated Since 0.2.0. Read files on the server with the Node.js entry and
 * check them before the upload with `excel-rs/detect`. Removed in a later major version.
 */
export async function writeXlsx(sheets: SheetSpec[]): Promise<Uint8Array> {
  return writeXlsxWith(await load(), sheets);
}
