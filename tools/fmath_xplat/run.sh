#!/bin/bash
# usage: tools/fmath_xplat/run.sh — crates/th08-core/src/fmath.rs on wasm32 (node) vs native x86_64, 10 functions × 2^20 inputs.
set -e
cd "$(dirname "$0")"
export CARGO_TARGET_DIR=../../target/fmath_xplat
cargo build -q --release
cargo build -q --release --lib --target wasm32-unknown-unknown
$CARGO_TARGET_DIR/release/fmath_xplat > $CARGO_TARGET_DIR/native.txt
node run.mjs $CARGO_TARGET_DIR/wasm32-unknown-unknown/release/fmath_xplat.wasm > $CARGO_TARGET_DIR/wasm.txt
n=$(wc -l < $CARGO_TARGET_DIR/native.txt)
if diff $CARGO_TARGET_DIR/native.txt $CARGO_TARGET_DIR/wasm.txt; then
  echo "fmath xplat: $n chunks × 65536 inputs identical (wasm32 vs native)"
else
  echo "fmath xplat: DIFFERS (function chunk above)"; exit 1
fi
