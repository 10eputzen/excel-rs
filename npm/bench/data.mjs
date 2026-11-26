// Test data: n rows × 10 columns of mixed types, as German users have them.
export function makeRows(n) {
  return Array.from({ length: n }, (_, i) => [
    `Kunde ${i}`, i, i * 1.25, i % 2 === 0, new Date(Date.UTC(2024, 0, 1 + (i % 365))),
    'Müllerstraße 12', i % 97, `Bestellung ${i}; Lieferung`, -i / 3, 'Köln',
  ]);
}

export function toCsv(rows) {
  const cell = (v) => {
    if (v instanceof Date) return v.toISOString().slice(0, 10);
    const s = String(v);
    return /[;"\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
  };
  return Buffer.from(rows.map((r) => r.map(cell).join(';')).join('\r\n'), 'utf8');
}
