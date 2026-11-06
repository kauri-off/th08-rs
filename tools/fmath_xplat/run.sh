#!/bin/bash
# usage: tools/fmath_xplat/run.sh — crates/th08-core/src/fmath.rs, 10 functions × 2^20 inputs, on wasm32 (node) and
# aarch64-linux-android (static, under qemu-aarch64) vs native x86_64. Canonical digests must match on every target;
# raw NaN bits (raw / nan lines) must match on wasm32, and on aarch64 only with FMATH_XPLAT_STRICT_NAN=1 (arm64 makes
# 0x7FC00000 where x86 makes 0xFFC00000). FMATH_XPLAT_ARM=0 skips aarch64. NDK: ANDROID_NDK_HOME, else the newest in
# $ANDROID_HOME / ~/.local/share/android-sdk/Sdk.
set -eu
cd "$(dirname "$0")"
export CARGO_TARGET_DIR=../../target/fmath_xplat
T=$CARGO_TARGET_DIR
cargo build -q --locked --release
cargo build -q --locked --release --lib --target wasm32-unknown-unknown
$T/release/fmath_xplat > $T/native.txt
node run.mjs $T/wasm32-unknown-unknown/release/fmath_xplat.wasm > $T/wasm.txt
targets=(wasm)
if [[ ${FMATH_XPLAT_ARM:-1} != 0 ]]; then
  SDK=${ANDROID_HOME:-$HOME/.local/share/android-sdk/Sdk}
  NDK=${ANDROID_NDK_HOME:-$(ls -d "$SDK"/ndk/*/ | sort -V | tail -1)}; NDK=${NDK%/}
  CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=$NDK/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android26-clang \
    CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS="-C target-feature=+crt-static" \
    cargo build -q --locked --release --bin fmath_xplat --target aarch64-linux-android
  qemu-aarch64 $T/aarch64-linux-android/release/fmath_xplat > $T/arm64.txt
  targets+=(arm64)
fi
n=$(grep -c '^[0-9]' $T/native.txt)
rc=0
for t in "${targets[@]}"; do
  if diff <(grep '^[0-9]' $T/native.txt) <(grep '^[0-9]' $T/$t.txt); then
    echo "fmath xplat: $n chunks × 65536 inputs identical ($t vs native)"
  else
    echo "fmath xplat: DIFFERS ($t vs native, function chunk above)"; rc=1
  fi
  # Differing raw lines as "raw: function K, N of 16 chunks" and "nan probe I: native → target".
  d=$(paste -d' ' <(grep -v '^[0-9]' $T/native.txt) <(grep -v '^[0-9]' $T/$t.txt) | awk '
    $1 == "raw" && $4 != $8 { n[$2]++ }
    $1 == "nan" && $3 != $6 { print "nan probe " $2 ": " $3 " -> " $6 }
    END { for (k in n) print "raw: function " k ", " n[k] " of 16 chunks" }' | sort)
  if [[ -z $d ]]; then
    echo "fmath xplat: raw NaN bits identical ($t vs native)"
  elif [[ $t == arm64 && ${FMATH_XPLAT_STRICT_NAN:-0} == 0 ]]; then
    echo "fmath xplat: raw NaN bits differ ($t vs native; known, not fatal):"; sed 's/^/  /' <<< "$d"
  else
    echo "fmath xplat: raw NaN bits DIFFER ($t vs native):"; sed 's/^/  /' <<< "$d"; rc=1
  fi
done
exit $rc
