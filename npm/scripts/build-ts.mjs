// Compiles the Node.js (CommonJS) and the browser (ES module) entries.
import { execFileSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { rmSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const pkg = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
rmSync(path.join(pkg, 'dist'), { recursive: true, force: true });
// Runs tsc through node: `npx` is a .cmd file on Windows, which execFile cannot start.
const tscBin = createRequire(import.meta.url).resolve('typescript/bin/tsc');
const tsc = (config) => execFileSync(process.execPath, [tscBin, '-p', config], { cwd: pkg, stdio: 'inherit' });
tsc('tsconfig.json');
tsc('tsconfig.browser.json');
writeFileSync(path.join(pkg, 'dist/browser/package.json'), '{ "type": "module" }\n');
