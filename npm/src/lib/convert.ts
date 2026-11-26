/**
 * Converts cell values to the types a schema asks for. Typed cells (xlsx, xls,
 * ods) are taken as they are; text cells, which is every csv cell, are parsed
 * in German notation by default (`1.234,56`, `31.12.2025`, `ja`/`nein`).
 */
import type { Cell } from './common';

/** Text of a cell, trimmed; `''` for an empty cell. */
export function cellText(v: Cell): string {
  if (v === null) return '';
  if (v instanceof Date) return isWholeDay(v) ? v.toISOString().slice(0, 10) : v.toISOString();
  return String(v).trim();
}

export function isEmpty(v: Cell): boolean {
  return v === null || (typeof v === 'string' && v.trim() === '');
}

/**
 * A number from a number cell or from text. `decimal` is the decimal
 * separator of text; the other one may group thousands in blocks of three.
 */
export function toNumber(v: Cell, decimal: ',' | '.'): number | undefined {
  if (typeof v === 'number') return Number.isFinite(v) ? v : undefined;
  if (typeof v !== 'string') return undefined;
  const group = decimal === ',' ? '.' : ',';
  const text = v.trim().replace(/[\s  ]/g, '');
  const [intPart, fraction, ...rest] = text.split(decimal);
  if (intPart === undefined || rest.length > 0) return undefined;
  const sign = /^[+-]/.test(intPart) ? intPart[0] : '';
  const digits = sign ? intPart.slice(1) : intPart;
  const grouped = new RegExp(`^\\d{1,3}(\\${group}\\d{3})+$`);
  if (!/^\d+$/.test(digits) && !grouped.test(digits)) return undefined;
  if (fraction !== undefined && !/^\d+$/.test(fraction)) return undefined;
  const n = Number(`${sign}${digits.split(group).join('')}${fraction === undefined ? '' : `.${fraction}`}`);
  return Number.isFinite(n) ? n : undefined;
}

const GERMAN_DATE = /^(\d{1,2})\.(\d{1,2})\.(\d{2}|\d{4})$/;
const ISO_DATE = /^(\d{4})-(\d{2})-(\d{2})$/;
const TIME = /^(\d{1,2}):(\d{2})(?::(\d{2}))?$/;

/**
 * A date (UTC) from a date cell or from text: `31.12.2025`, `31.12.25`
 * (20xx), `2025-12-31`, each optionally followed by `HH:MM[:SS]` after a
 * space (or `T` for the ISO form). Impossible dates such as 31.02. fail.
 */
export function toDate(v: Cell): Date | undefined {
  if (v instanceof Date) return Number.isNaN(v.getTime()) ? undefined : v;
  if (typeof v !== 'string') return undefined;
  const [datePart = '', timePart, ...rest] = v.trim().split(/[ T]+/);
  if (rest.length > 0) return undefined;
  let y: number, m: number, d: number;
  const german = GERMAN_DATE.exec(datePart);
  const iso = ISO_DATE.exec(datePart);
  if (german) {
    [d, m, y] = [Number(german[1]), Number(german[2]), Number(german[3])];
    if (german[3]!.length === 2) y += 2000;
  } else if (iso) {
    [y, m, d] = [Number(iso[1]), Number(iso[2]), Number(iso[3])];
  } else {
    return undefined;
  }
  let [h, min, s] = [0, 0, 0];
  if (timePart !== undefined) {
    const t = TIME.exec(timePart);
    if (!t) return undefined;
    [h, min, s] = [Number(t[1]), Number(t[2]), Number(t[3] ?? 0)];
    if (h > 23 || min > 59 || s > 59) return undefined;
  }
  const date = new Date(Date.UTC(y, m - 1, d, h, min, s));
  // Date.UTC rolls 31.02. over into March; such input is rejected.
  if (date.getUTCFullYear() !== y || date.getUTCMonth() !== m - 1 || date.getUTCDate() !== d) return undefined;
  return date;
}

const TRUE = ['ja', 'j', 'wahr', 'true', 'x', 'yes', '1'];
const FALSE = ['nein', 'n', 'falsch', 'false', 'no', '0'];

/** A boolean from a boolean cell, 1/0 or text such as `ja`/`nein`, `wahr`/`falsch`, `x`. */
export function toBoolean(v: Cell): boolean | undefined {
  if (typeof v === 'boolean') return v;
  if (v === 1 || v === 0) return v === 1;
  if (typeof v !== 'string') return undefined;
  const t = v.trim().toLowerCase();
  if (TRUE.includes(t)) return true;
  if (FALSE.includes(t)) return false;
  return undefined;
}

function isWholeDay(d: Date): boolean {
  return d.getTime() % 86_400_000 === 0;
}

/** `31.12.2025` (and ` 14:30` if the time is not midnight), for messages. */
export function formatDate(d: Date): string {
  const p = (n: number) => String(n).padStart(2, '0');
  const day = `${p(d.getUTCDate())}.${p(d.getUTCMonth() + 1)}.${d.getUTCFullYear()}`;
  return isWholeDay(d) ? day : `${day} ${p(d.getUTCHours())}:${p(d.getUTCMinutes())}`;
}

/** A number in German notation, for messages. */
export function formatNumber(n: number): string {
  return String(n).replace('.', ',');
}
