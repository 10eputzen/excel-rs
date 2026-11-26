// Runs inside Chromium for browser.e2e.test.mjs.
const run = async () => {
  const { readSheet, writeXlsx } = await import('/dist/browser/browser.js');
  const bytes = new Uint8Array(await (await fetch('/fixtures/reference-libreoffice.xlsx')).arrayBuffer());
  const res = await readSheet(bytes);
  const roundTrip = (await readSheet(await writeXlsx([{ name: 'S', columns: [{ header: 'a' }], rows: [[1]] }]))).rows;
  let errorCode;
  await readSheet(new TextEncoder().encode('PK\u0003\u0004broken')).catch((e) => (errorCode = e.code));
  return {
    sheetNames: res.sheetNames,
    row: res.rows[1].map((v) => (v instanceof Date ? v.toISOString() : v)),
    roundTrip,
    errorCode,
  };
};
run().then(
  (r) => (window.__result = r),
  (e) => (window.__result = { error: String(e) }),
);
