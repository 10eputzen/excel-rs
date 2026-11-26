// Builds the Rust crate for wasm32 and generates the Node.js bindings in ./wasm.
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const run = (cmd, args) => execFileSync(cmd, args, { cwd: root, stdio: 'inherit' });

run('cargo', ['build', '-p', 'excel-rs-wasm', '--release', '--locked', '--target', 'wasm32-unknown-unknown']);
run('wasm-bindgen', [
  'target/wasm32-unknown-unknown/release/excel_rs_wasm.wasm',
  '--target', 'nodejs',
  '--out-dir', 'npm/wasm',
]);
