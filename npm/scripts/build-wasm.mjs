// Builds the Rust crate for wasm32 and generates the bindings:
//   wasm/node  CommonJS, loads the binary synchronously (Node.js entry)
//   wasm/web   ES module, fetches the binary on init (browser entry)
import { execFileSync } from 'node:child_process';
import { writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const run = (cmd, args) => execFileSync(cmd, args, { cwd: root, stdio: 'inherit' });

run('cargo', ['build', '-p', 'excel-rs-wasm', '--release', '--locked', '--target', 'wasm32-unknown-unknown']);
for (const [target, dir] of [['nodejs', 'node'], ['web', 'web']]) {
  run('wasm-bindgen', [
    'target/wasm32-unknown-unknown/release/excel_rs_wasm.wasm',
    '--target', target,
    '--out-dir', `npm/wasm/${dir}`,
    '--experimental-reset-state-function',
  ]);
}
writeFileSync(path.join(root, 'npm/wasm/web/package.json'), '{ "type": "module" }\n');
