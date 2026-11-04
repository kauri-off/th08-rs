#!/bin/bash
# usage: tools/android/cargo_android.sh OUT_DIR ABI... — libth08.so per ABI (arm64-v8a, x86_64) built with the NDK
# clang into OUT_DIR/<abi>/ (debug info stripped). Run by the Gradle task cargoBuild (android/app/build.gradle.kts).
# Env: ANDROID_SDK, TH08_ANDROID_MIN_SDK (NDK clang API, default 26), TH08_ANDROID_PROFILE (cargo profile, default release).
set -euo pipefail
out=$(realpath -m "$1"); shift
cd "$(dirname "$0")/../.."
SDK=${ANDROID_SDK:-$HOME/.local/share/android-sdk/Sdk}
API=${TH08_ANDROID_MIN_SDK:-26}
PROFILE=${TH08_ANDROID_PROFILE:-release}
NDK=${ANDROID_NDK_HOME:-$(ls -d "$SDK"/ndk/*/ | sort -V | tail -1)}; NDK=${NDK%/}
NBIN=$NDK/toolchains/llvm/prebuilt/linux-x86_64/bin
arch_of() { case $1 in arm64-v8a) echo aarch64 ;; x86_64) echo x86_64 ;; *) echo "unknown ABI $1" >&2; exit 1 ;; esac; }
rm -rf "$out"
for abi in "$@"; do
  triple=$(arch_of "$abi")-linux-android
  up=$(tr 'a-z-' 'A-Z_' <<< "$triple")
  rustup target list --installed | grep -qx "$triple" || rustup target add "$triple"
  env "CC_${triple//-/_}=$NBIN/$triple$API-clang" "CXX_${triple//-/_}=$NBIN/$triple$API-clang++" \
    "AR_${triple//-/_}=$NBIN/llvm-ar" "CARGO_TARGET_${up}_LINKER=$NBIN/$triple$API-clang" \
    ANDROID_NDK_HOME="$NDK" ANDROID_NDK_ROOT="$NDK" \
    cargo build --profile "$PROFILE" --lib -p th08 --target "$triple"
  dir=$PROFILE; [[ $PROFILE == dev ]] && dir=debug
  mkdir -p "$out/$abi"
  cp "target/$triple/$dir/libth08.so" "$out/$abi/"
  "$NBIN/llvm-strip" --strip-debug "$out/$abi/libth08.so"
done
