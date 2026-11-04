#!/bin/bash
# usage: tools/android/build_apk.sh [--abi ABI]... [--data DIR | --packed DIR] (--keystore PROPS | --debug-key) [--variant V] [--out APK]
# Thin wrapper over `cargo xtask dist --target android-apk` (xtask/src/android.rs); --data DIR = --assets raw --data DIR;
# --packed DIR = --assets packed --data DIR (th08.pak + thbgm.pak, cached in target/pack/).
# Default --out target/android/th08-<variant>.apk.
set -euo pipefail
cd "$(dirname "$0")/../.."
args=() variant=release out=
while [[ $# -gt 0 ]]; do
  case $1 in
    --data) args+=(--assets raw --data "$2"); shift 2 ;;
    --packed) args+=(--assets packed --data "$2"); shift 2 ;;
    --variant) variant=$2; args+=(--variant "$2"); shift 2 ;;
    --out) out=$2; shift 2 ;;
    -h|--help) sed -n '2,5p' "$0"; exit 0 ;;
    *) args+=("$1"); shift ;;
  esac
done
exec cargo xtask dist --target android-apk "${args[@]}" --out "${out:-target/android/th08-$variant.apk}"
