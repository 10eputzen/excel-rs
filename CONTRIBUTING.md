# Contributing

Thanks for your interest. Bug reports, sample files that are read incorrectly and small fixes are very welcome. For larger features, please open an issue first so we can agree on the approach before you spend time on it.

## Setup

See [Development](README.md#development) in the README for the toolchain (Rust with the `wasm32-unknown-unknown` target, `wasm-bindgen-cli` and Node 20 or newer).

## Before you open a pull request

```sh
cargo fmt --all --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cd npm && npm run build && npm run typecheck && npm test
```

CI runs the same checks on Linux and Windows for every pull request.

## Guidelines

- The reader must never panic on a broken file. If you touch reading code, add a test with a file that shows the problem; broken files go to `fixtures/broken/`, files saved by real spreadsheet programs to `fixtures/excel/` (with their source and license in `SOURCES.md`).
- Changes to the public API come with TypeScript types, a test and a `CHANGELOG.md` entry.
- Bug fixes come with a test that fails without the fix.
- Test fixtures must not contain personal or confidential data.

By contributing you agree that your contribution is licensed under the [MIT license](LICENSE), without any additional terms, and that you follow the [Code of Conduct](CODE_OF_CONDUCT.md).
