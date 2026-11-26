/**
 * Checks a sheet against a schema: finds the columns by their header, converts
 * every cell to the column type and collects errors with German messages, so
 * an app can show a preview before it writes anything.
 */
import { ExcelError } from './common';
import type { Cell } from './common';
import { normalizeHeader as normalize } from './detect';
import { cellText, formatDate, formatNumber, isEmpty, toBoolean, toDate, toNumber } from './convert';

export type ColumnType = 'string' | 'number' | 'integer' | 'date' | 'boolean';

type ValueOf<T extends ColumnType> = T extends 'string'
  ? string
  : T extends 'number' | 'integer'
    ? number
    : T extends 'date'
      ? Date
      : boolean;

export interface ColumnRule<T extends ColumnType = ColumnType> {
  /** Header text in the first row; compared ignoring case and extra spaces. */
  header: string;
  /** Other header texts that name the same column. */
  aliases?: readonly string[];
  /** Default `string`. */
  type?: T;
  /** The column must exist and no cell may be empty. */
  required?: boolean;
  /** Smallest value (number, integer, date) or shortest length (string). */
  min?: number | Date;
  /** Largest value (number, integer, date) or longest length (string). */
  max?: number | Date;
  /** The text of the cell must match. */
  pattern?: RegExp;
  /** Allowed values. Text is matched ignoring case and returned as listed here. */
  values?: readonly (string | number | boolean)[];
  /** Decimal separator of numbers written as text, as in csv files. Default `,`. */
  decimal?: ',' | '.';
  /** Own check after all others pass: return a message to reject the value. */
  validate?: (value: ValueOf<T>) => string | null | undefined;
}

type RuleOf<T extends ColumnType> = Omit<ColumnRule<T>, 'type'> & { type: T };

/** A column rule; the `type` decides what `validate` receives. */
export type AnyColumnRule =
  | (Omit<ColumnRule<'string'>, 'type'> & { type?: 'string' })
  | RuleOf<'number'>
  | RuleOf<'integer'>
  | RuleOf<'date'>
  | RuleOf<'boolean'>;

/** Column rules by the property name the value gets in each row object. */
export type Schema = Record<string, AnyColumnRule>;

type TypeOf<R> = R extends { type: infer T extends ColumnType } ? T : 'string';
type RowValue<R> = R extends { required: true } ? ValueOf<TypeOf<R>> : ValueOf<TypeOf<R>> | null;

/** A row as an object; optional columns are `null` when empty or missing. */
export type SchemaRow<S extends Schema> = { [K in keyof S]: RowValue<S[K]> };

export type SchemaErrorCode =
  | 'MISSING_COLUMN'
  | 'REQUIRED'
  | 'INVALID_TYPE'
  | 'BELOW_MIN'
  | 'ABOVE_MAX'
  | 'PATTERN'
  | 'NOT_ALLOWED'
  | 'CUSTOM';

export interface SchemaError {
  /** Row number as Excel shows it: the header is row 1. */
  row: number;
  /** Header of the column as written in the schema. */
  column: string;
  /** Cell address such as `B7`; missing for `MISSING_COLUMN`. */
  cell?: string;
  code: SchemaErrorCode;
  /** German message for the user. */
  message: string;
}

export interface SchemaResult<S extends Schema> {
  /** Rows without errors, in file order. Completely empty rows are skipped. */
  rows: SchemaRow<S>[];
  /** At most `maxErrors` errors, in file order. */
  errors: SchemaError[];
  /** True when more errors were found than `errors` holds. */
  errorsTruncated: boolean;
  /** Number of data rows read, with and without errors (empty rows not counted). */
  rowCount: number;
}

const TYPES: readonly ColumnType[] = ['string', 'number', 'integer', 'date', 'boolean'];
const NOT_A: Record<ColumnType, string> = {
  string: 'kein Text',
  number: 'keine Zahl',
  integer: 'keine ganze Zahl',
  date: 'kein gültiges Datum',
  boolean: 'kein Ja/Nein-Wert',
};

/** Throws INVALID_INPUT for a schema that cannot be applied. */
export function validateSchema(schema: unknown): asserts schema is Schema {
  const fail = (msg: string) => {
    throw new ExcelError('INVALID_INPUT', `schema: ${msg}`);
  };
  if (schema === null || typeof schema !== 'object' || Array.isArray(schema)) fail('must be an object');
  const entries = Object.entries(schema as Record<string, unknown>);
  if (entries.length === 0) fail('has no columns');
  for (const [key, rule] of entries) {
    if (key === '__proto__') fail('__proto__ is not allowed as a column key');
    if (rule === null || typeof rule !== 'object') fail(`${key} must be an object`);
    const r = rule as ColumnRule;
    if (typeof r.header !== 'string' || normalize(r.header) === '') fail(`${key}.header must be a non-empty string`);
    if (r.aliases !== undefined && (!Array.isArray(r.aliases) || r.aliases.some((a) => typeof a !== 'string'))) {
      fail(`${key}.aliases must be an array of strings`);
    }
    if (r.type !== undefined && !TYPES.includes(r.type)) fail(`${key}.type must be one of ${TYPES.join(', ')}`);
    for (const bound of ['min', 'max'] as const) {
      const b = r[bound];
      if (b !== undefined && !(typeof b === 'number' || b instanceof Date)) fail(`${key}.${bound} must be a number or Date`);
    }
    if (r.pattern !== undefined && !(r.pattern instanceof RegExp)) fail(`${key}.pattern must be a RegExp`);
    if (r.values !== undefined && !Array.isArray(r.values)) fail(`${key}.values must be an array`);
    if (r.decimal !== undefined && r.decimal !== ',' && r.decimal !== '.') fail(`${key}.decimal must be "," or "."`);
    if (r.validate !== undefined && typeof r.validate !== 'function') fail(`${key}.validate must be a function`);
  }
}

/** Applies a validated schema to sheet rows (first row = header). */
export function applySchema<S extends Schema>(rows: Cell[][], schema: S, maxErrors: number): SchemaResult<S> {
  const result: SchemaResult<S> = { rows: [], errors: [], errorsTruncated: false, rowCount: 0 };
  const report = (e: SchemaError) => {
    if (result.errors.length < maxErrors) result.errors.push(e);
    else result.errorsTruncated = true;
  };
  const columns = findColumns(rows[0] ?? [], schema);
  for (const [key, index] of columns) {
    const rule = schema[key]!;
    if (index === -1 && rule.required) {
      report({ row: 1, column: rule.header, code: 'MISSING_COLUMN', message: `Die Spalte „${rule.header}“ fehlt.` });
    }
  }
  if (result.errors.length > 0) return result;

  for (let r = 1; r < rows.length; r++) {
    const cells = rows[r]!;
    if (cells.every(isEmpty)) continue;
    result.rowCount++;
    const row: Record<string, unknown> = {};
    let ok = true;
    for (const [key, index] of columns) {
      const rule = schema[key]!;
      const cell = index === -1 ? null : (cells[index] ?? null);
      const checked = checkCell(cell, rule);
      if ('value' in checked) {
        row[key] = checked.value;
      } else {
        ok = false;
        report({ row: r + 1, column: rule.header, cell: `${columnLetter(index)}${r + 1}`, ...checked });
      }
    }
    if (ok) result.rows.push(row as SchemaRow<S>);
  }
  return result;
}

/** Column index of each schema key, -1 if the header is missing. First match wins. */
function findColumns(header: Cell[], schema: Schema): [string, number][] {
  const names = header.map((h) => normalize(cellText(h)));
  return Object.entries(schema).map(([key, rule]) => {
    const wanted = [rule.header, ...(rule.aliases ?? [])].map(normalize);
    return [key, names.findIndex((n) => n !== '' && wanted.includes(n))];
  });
}

type Failure = { code: SchemaErrorCode; message: string };
type Checked = { value: unknown } | Failure;

/** The converted value, or the reason the cell is rejected. */
function checkCell(cell: Cell, rule: AnyColumnRule): Checked {
  const name = `„${rule.header}“`;
  if (isEmpty(cell)) {
    return rule.required ? fail('REQUIRED', `${name} darf nicht leer sein.`) : { value: null };
  }
  const type = rule.type ?? 'string';
  const value = convert(cell, type, rule.decimal ?? ',');
  const shown = `„${shorten(cellText(cell))}“`;
  if (value === undefined) return fail('INVALID_TYPE', `${name}: ${shown} ist ${NOT_A[type]}.`);

  const size = type === 'string' ? (value as string).length : (value as number | Date).valueOf();
  if (rule.min !== undefined && size < rule.min.valueOf()) {
    return fail('BELOW_MIN', `${name}: ${shown} ${outOfRange(type, 'min', rule.min)}.`);
  }
  if (rule.max !== undefined && size > rule.max.valueOf()) {
    return fail('ABOVE_MAX', `${name}: ${shown} ${outOfRange(type, 'max', rule.max)}.`);
  }
  if (rule.pattern !== undefined) {
    rule.pattern.lastIndex = 0;
    if (!rule.pattern.test(type === 'string' ? (value as string) : cellText(cell))) {
      return fail('PATTERN', `${name}: ${shown} hat nicht das erwartete Format.`);
    }
  }
  let out = value;
  if (rule.values !== undefined) {
    const match = rule.values.find((a) => sameValue(a, value));
    if (match === undefined) {
      const list = rule.values.slice(0, 10).map((a) => `„${String(a)}“`).join(', ');
      const more = rule.values.length > 10 ? ', …' : '';
      return fail('NOT_ALLOWED', `${name}: ${shown} ist nicht erlaubt (erlaubt: ${list}${more}).`);
    }
    out = match;
  }
  const custom = rule.validate?.(out as never);
  if (typeof custom === 'string' && custom !== '') return fail('CUSTOM', `${name}: ${custom}`);
  return { value: out };
}

/** „ist kleiner als 5“, „ist kürzer als 3 Zeichen“, „liegt vor dem 01.01.2025“. */
function outOfRange(type: ColumnType, side: 'min' | 'max', bound: number | Date): string {
  if (type === 'string') return `ist ${side === 'min' ? 'kürzer' : 'länger'} als ${formatNumber(Number(bound))} Zeichen`;
  if (type === 'date') return `liegt ${side === 'min' ? 'vor dem' : 'nach dem'} ${formatDate(new Date(bound.valueOf()))}`;
  return `ist ${side === 'min' ? 'kleiner' : 'größer'} als ${formatNumber(Number(bound))}`;
}

function convert(cell: Cell, type: ColumnType, decimal: ',' | '.'): unknown {
  switch (type) {
    case 'string':
      return cellText(cell);
    case 'number':
      return toNumber(cell, decimal);
    case 'integer': {
      const n = toNumber(cell, decimal);
      return n !== undefined && Number.isSafeInteger(n) ? n : undefined;
    }
    case 'date':
      return toDate(cell);
    case 'boolean':
      return toBoolean(cell);
  }
}

function sameValue(allowed: string | number | boolean, value: unknown): boolean {
  if (typeof allowed === 'string' && typeof value === 'string') return allowed.toLowerCase() === value.toLowerCase();
  if (value instanceof Date) return false;
  return allowed === value;
}

function fail(code: SchemaErrorCode, message: string): Failure {
  return { code, message };
}

function shorten(s: string): string {
  return s.length > 40 ? `${s.slice(0, 39)}…` : s;
}

/** 0 → A, 25 → Z, 26 → AA. */
function columnLetter(index: number): string {
  if (index < 0) return '';
  let s = '';
  for (let n = index + 1; n > 0; n = Math.floor((n - 1) / 26)) s = String.fromCharCode(65 + ((n - 1) % 26)) + s;
  return s;
}
