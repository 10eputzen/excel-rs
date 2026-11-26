// Compile-time checks of the schema row types (npm run typecheck).
import { readSheet } from '../src/lib/index';
import type { Schema } from '../src/lib/index';
const r = readSheet(new Uint8Array(), { schema: { a: { header: 'A', type: 'number', required: true }, b: { header: 'B', type: 'date' }, c: { header: 'C' } } });
const a: number = r.rows[0]!.a;
const b: Date | null = r.rows[0]!.b;
const c: string | null = r.rows[0]!.c;
// @ts-expect-error b may be null
const b2: Date = r.rows[0]!.b;
const plain = readSheet(new Uint8Array());
const cells = plain.rows;
void [a, b, c, b2, cells];

// A custom check gets the converted value of its column type.
readSheet(new Uint8Array(), {
  schema: {
    email: { header: 'E-Mail', validate: (v) => (v.includes('@') ? undefined : 'ist keine E-Mail-Adresse.') },
    qty: { header: 'Menge', type: 'integer', validate: (n) => (n > 0 ? undefined : 'muss positiv sein.') },
  },
});

// A schema in its own variable keeps literal types with `satisfies`.
const orders = { qty: { header: 'Menge', type: 'integer', required: true } } satisfies Schema;
const qty: number = readSheet(new Uint8Array(), { schema: orders }).rows[0]!.qty;
void qty;
