#!/bin/bash
# usage: tools/android/run_debug.sh [build|emulator|install|run|shot|stop|all]...  (default: build)
# Debug APK (ARCH_D_MOBILE §6.3 run 1, §6.4): the Gradle debug variant of android/ (Compose launcher + NativeActivity
# game) for arm64-v8a + x86_64, signed with the throwaway key in target/android/; release builds: cargo xtask dist.
#   build     cargo xtask dist --variant debug → target/android/th08-debug.apk (DATA=DIR bundles th08.dat +
#             thbgm.dat, PACKED=DIR the repacked th08.pak + thbgm.pak)
#   emulator  start a headless emulator (own AVD th08dbg, no window, no audio) and wait for boot
#   install   adb install on the emulator only (EMU_SERIAL, never a physical device)
#   run       start the Compose launcher, wait, dump logcat to target/android/logcat.txt
#   shot      adb exec-out screencap → target/android/screen.png
#   stop      kill the emulator
# Env: ANDROID_SDK (default ~/.local/share/android-sdk/Sdk), ANDROID_API (min SDK / NDK clang API, default 26),
#      EMU_PORT (default 5584), PROFILE (release|dev, default release), TARGETS (default "aarch64 x86_64"), DATA,
#      PACKED.
set -euo pipefail
cd "$(dirname "$0")/../.."
SDK=${ANDROID_SDK:-$HOME/.local/share/android-sdk/Sdk}
PKG=org.th08.port.debug
OUT=target/android
EMU_PORT=${EMU_PORT:-5584}
EMU_SERIAL=emulator-$EMU_PORT
export PROFILE=${PROFILE:-release}
TARGETS=${TARGETS:-aarch64 x86_64}
ADB=$SDK/platform-tools/adb
mkdir -p "$OUT"

abi_of() { case $1 in aarch64) echo arm64-v8a ;; x86_64) echo x86_64 ;; *) echo "unknown target $1" >&2; exit 1 ;; esac; }

# Gradle debug variant (org.th08.port.debug) through cargo xtask dist; DATA=DIR bundles the game files.
build() {
  local abis=() a
  for a in $TARGETS; do abis+=(--abi "$(abi_of "$a")"); done
  cargo xtask dist --target android-apk --variant debug --debug-key "${abis[@]}" \
    ${DATA:+--assets raw --data "$DATA"} ${PACKED:+--assets packed --data "$PACKED"} --out "$OUT/th08-debug.apk"
}

emu_adb() { "$ADB" -s "$EMU_SERIAL" "$@"; }

emulator_start() {
  local img
  img=$(ls -d "$SDK"/system-images/*/*/x86_64/ | sort -V | tail -1)
  [[ -n $img ]] || { echo "no x86_64 system image under $SDK/system-images"; exit 1; }
  # avdmanager ignores ANDROID_AVD_HOME, so the AVD lives in the default AVD home under its own name.
  if ! "$SDK"/cmdline-tools/*/bin/avdmanager list avd -c 2> /dev/null | grep -qx th08dbg; then
    local rel=${img#"$SDK"/}
    echo no | "$SDK"/cmdline-tools/*/bin/avdmanager create avd -n th08dbg -k "$(tr / ';' <<< "${rel%/}")" -d pixel_7 > /dev/null
  fi
  "$ADB" start-server > /dev/null
  # No window, no host audio, software GPU.
  env -u WAYLAND_DISPLAY "$SDK/emulator/emulator" -avd th08dbg -port "$EMU_PORT" -no-window -no-audio \
    -no-boot-anim -no-snapshot -gpu swiftshader_indirect > "$OUT/emulator.log" 2>&1 &
  echo "emulator pid $! on $EMU_SERIAL"
  emu_adb wait-for-device
  until [[ $(emu_adb shell getprop sys.boot_completed 2> /dev/null | tr -d '\r') == 1 ]]; do sleep 2; done
  echo "emulator booted"
}

install() { emu_adb install -r "$OUT/th08-debug.apk"; }

run() {
  emu_adb logcat -c
  emu_adb shell am start -n "$PKG/org.th08.port.LauncherActivity"
  sleep "${RUN_WAIT:-15}"
  emu_adb logcat -d > "$OUT/logcat.txt"
  grep -E "th08|NativeActivity|wgpu|panic|FATAL" "$OUT/logcat.txt" | tail -40 || true
}

shot() { emu_adb exec-out screencap -p > "$OUT/screen.png"; ls -l "$OUT/screen.png"; }

stop() { emu_adb emu kill 2> /dev/null || true; }

[[ $# == 0 ]] && set -- build
for c in "$@"; do
  case $c in
    build) build ;;
    emulator) emulator_start ;;
    install) install ;;
    run) run ;;
    shot) shot ;;
    stop) stop ;;
    all) build; emulator_start; install; run; shot ;;
    *) echo "unknown command $c"; exit 1 ;;
  esac
done
