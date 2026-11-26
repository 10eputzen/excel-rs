#!/usr/bin/env sh
# Builds the seed corpus for the `read` target from the fixtures: each file
# once per format selector (first byte, see fuzz_targets/read.rs).
set -eu
cd "$(dirname "$0")"
mkdir -p corpus/read
for f in ../fixtures/*.* ../fixtures/excel/*.*; do
  case "$f" in *.json|*.md) continue ;; esac
  [ -f "$f" ] || continue
  name=$(basename "$f")
  for sel in 0 1 2 3 4 5; do
    { printf "\\$(printf '%03o' "$sel")"; cat "$f"; } > "corpus/read/$sel-$name"
  done
done
