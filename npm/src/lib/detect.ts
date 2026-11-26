/**
 * excel-rs/detect: checks in the browser before an upload, without WebAssembly.
 * Pass the first bytes of a file (4 KiB are plenty) or the start of a csv text.
 */

export type DetectedFormat = 'xlsx' | 'ods' | 'xls' | 'csv' | 'unknown';

/**
 * Format of a file by its first bytes. Every zip archive that is not an ods
 * file counts as `xlsx` (xlsm and xlsb are zip archives too); readable text
 * counts as `csv`.
 */
export function detectFormat(bytes: Uint8Array): DetectedFormat {
  const starts = (sig: number[]) => sig.every((b, i) => bytes[i] === b);
  if (starts([0x50, 0x4b, 0x03, 0x04])) {
    // ods stores the uncompressed entry "mimetype" first.
    return latin1(bytes.subarray(30, 100)).includes('opendocument.spreadsheet') ? 'ods' : 'xlsx';
  }
  if (starts([0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1])) return 'xls';
  if (starts([0xff, 0xfe]) || starts([0xfe, 0xff])) return 'csv';
  if (bytes.length === 0 || starts([0x25, 0x50, 0x44, 0x46])) return 'unknown';
  let control = 0;
  for (const b of bytes) {
    if (b === 0) return 'unknown';
    if (b < 32 && b !== 9 && b !== 10 && b !== 13) control++;
  }
  return control * 100 > bytes.length ? 'unknown' : 'csv';
}

export interface CsvHeader {
  delimiter: ',' | ';' | '\t' | '|';
  /** Header cells, trimmed, quotes removed. */
  columns: string[];
}

/**
 * Header row of csv text: the delimiter that occurs most often outside quotes
 * in the first line, and the cells of that line.
 */
export function readCsvHeader(text: string): CsvHeader {
  const line = firstLine(text.replace(/^\ufeff/, ''));
  let delimiter: CsvHeader['delimiter'] = ';';
  let best = 0;
  for (const d of [';', ',', '\t', '|'] as const) {
    const n = split(line, d).length;
    if (n > best) {
      delimiter = d;
      best = n;
    }
  }
  return { delimiter, columns: split(line, delimiter).map((c) => c.trim()) };
}

/**
 * Headers of required schema columns that `columns` lacks (compared like
 * `readSheet` does: ignoring case and extra spaces, aliases count).
 */
export function missingColumns(
  columns: readonly string[],
  schema: Record<string, { header: string; aliases?: readonly string[]; required?: boolean }>,
): string[] {
  const present = columns.map(normalizeHeader);
  return Object.values(schema)
    .filter((r) => r.required && ![r.header, ...(r.aliases ?? [])].some((h) => present.includes(normalizeHeader(h))))
    .map((r) => r.header);
}

/** Header text for comparison: no BOM, NFC, trimmed, single spaces, lower case. */
export function normalizeHeader(s: string): string {
  return s.replace(/^\ufeff/, '').normalize('NFC').trim().replace(/\s+/g, ' ').toLowerCase();
}

/** Text up to the first line break outside quotes. */
function firstLine(text: string): string {
  let quoted = false;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (c === '"') quoted = !quoted;
    else if (!quoted && (c === '\n' || c === '\r')) return text.slice(0, i);
  }
  return text;
}

/** Cells of one csv line; `""` inside quotes is a quote. */
function split(line: string, delimiter: string): string[] {
  const cells: string[] = [];
  let cell = '';
  let quoted = false;
  for (let i = 0; i < line.length; i++) {
    const c = line[i];
    if (c === '"') {
      if (quoted && line[i + 1] === '"') cell += line[++i];
      else quoted = !quoted;
    } else if (c === delimiter && !quoted) {
      cells.push(cell);
      cell = '';
    } else {
      cell += c;
    }
  }
  cells.push(cell);
  return cells;
}

function latin1(bytes: Uint8Array): string {
  return String.fromCharCode(...bytes);
}
