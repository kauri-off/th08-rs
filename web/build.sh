#!/usr/bin/env bash
# Builds the web version into web/dist (serve that directory over http): wrapper over `cargo xtask dist --target web`.
set -e
cd "$(dirname "$0")/.."
exec cargo xtask dist --target web --assets none --out web/dist
