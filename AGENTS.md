# AGENTS.md

Guidance for coding agents working on excel-rs. Human contributors: see [CONTRIBUTING.md](CONTRIBUTING.md), which this file follows.

## Project

Spreadsheet reader and writer in Rust, compiled to WebAssembly and published to npm as `@10eputzen/excel-rs`.

- `crates/core`: reading and writing.
- `crates/wasm`: wasm-bindgen bindings.
- `npm/`: TypeScript wrapper, tests and benchmarks.
- `fuzz/`: fuzz targets.

Toolchain: see [Development](README.md#development) in the README.

## Checks

Run before every push; CI runs the same on Linux and Windows:

```sh
cargo fmt --all --check
cargo clippy --all-targets --locked -- -D warnings
cargo clippy -p excel-rs-wasm --target wasm32-unknown-unknown --locked -- -D warnings
cargo test --locked
cd npm && npm run build && npm run typecheck && npm test
```

## Rules

- The reader must never panic on a broken file. Changes to reading code come with a test file that shows the problem: broken files in `fixtures/broken/`, files from real spreadsheet programs in `fixtures/excel/` (source and license in `SOURCES.md`).
- Changes to the public API come with TypeScript types, a test and a `CHANGELOG.md` entry.
- Bug fixes come with a test that fails without the fix.
- Test fixtures must not contain personal or confidential data.
- Commits use a GitHub noreply address. No email addresses, local paths or machine names in code, commits or pull requests.

## Release

Bump `version` in `npm/package.json` and add the `CHANGELOG.md` section in a pull request. After it is merged, start `.github/workflows/release.yml` by hand: it publishes to npm through trusted publishing and creates the tag `v<version>` and the GitHub release. No npm token is stored. Do not push release tags by hand.
