#!/usr/bin/env bash
# Builds the web version into web/dist (serve that directory over http).
set -e
cd "$(dirname "$0")/.."
cargo build --lib --release --target wasm32-unknown-unknown
wasm-bindgen --target web --no-typescript --out-dir web/dist target/wasm32-unknown-unknown/release/th08.wasm
cp web/index.html web/dist/
