// Runs inside Chromium for browser.e2e.test.mjs.
const loadFailure = async () => {
  const { initExcel, readSheet } = await import('/dist/browser/browser.js');
  let code;
  await initExcel('/does-not-exist.wasm').catch((e) => (code = e.code));
  // A later call retries with the default location and succeeds.
  const rows = (await readSheet(new TextEncoder().encode('a;b'))).rows;
  return { code, rows };
};

const run = async () => {
  const { readSheet, writeXlsx } = await import('/dist/browser/browser.js');
  const bytes = new Uint8Array(await (await fetch('/fixtures/reference-libreoffice.xlsx')).arrayBuffer());
  const res = await readSheet(bytes);
  const roundTrip = (await readSheet(await writeXlsx([{ name: 'S', columns: [{ header: 'a' }], rows: [[1]] }]))).rows;
  let errorCode;
  await readSheet(new TextEncoder().encode('PK\u0003\u0004broken')).catch((e) => (errorCode = e.code));
  // A prefix of the xls fixture crashes the parser (WebAssembly trap); the next call must work.
  const xls = new Uint8Array(await (await fetch('/fixtures/reference-libreoffice.xls')).arrayBuffer());
  let trapCode;
  await readSheet(xls.subarray(0, 513)).catch((e) => (trapCode = e.code));
  const afterTrap = (await readSheet(xls)).rows[3][5];
  return {
    sheetNames: res.sheetNames,
    row: res.rows[1].map((v) => (v instanceof Date ? v.toISOString() : v)),
    roundTrip,
    errorCode,
    trapCode,
    afterTrap,
  };
};
(location.search === '?loadfail' ? loadFailure() : run()).then(
  (r) => (window.__result = r),
  (e) => (window.__result = { error: String(e) }),
);
