// Runs one library on one task in a fresh process and prints JSON:
// median time of RUNS runs after one warm-up, and peak memory of the process.
import { readFileSync } from 'node:fs';
import { LIBS } from './libs.mjs';
import { makeRows } from './data.mjs';

const [lib, task, file, rowsArg, runsArg] = process.argv.slice(2);
const RUNS = Number(runsArg ?? 5);
const api = await LIBS[lib].load();
const fn = api[task];
if (!fn) {
  console.log(JSON.stringify({ unsupported: true }));
  process.exit(0);
}
const input = task === 'writeXlsx' ? makeRows(Number(rowsArg)) : readFileSync(file);
const times = [];
let out;
for (let i = 0; i <= RUNS; i++) {
  const t = performance.now();
  out = await fn(input);
  if (i > 0) times.push(performance.now() - t);
}
times.sort((a, b) => a - b);
const rows = task === 'writeXlsx' ? undefined : out.length;
console.log(JSON.stringify({ ms: times[Math.floor(times.length / 2)], peakMb: process.resourceUsage().maxRSS / 1024, rows }));
