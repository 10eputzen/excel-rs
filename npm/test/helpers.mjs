import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
export const lib = require('../dist/index.js');
export const fixturesDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../fixtures');
export const fixture = (name) => readFileSync(path.join(fixturesDir, name));

/** Turns rows into JSON-comparable values (Dates → ISO strings). */
export const plain = (rows) => rows.map((r) => r.map((v) => (v instanceof Date ? v.toISOString() : v)));
