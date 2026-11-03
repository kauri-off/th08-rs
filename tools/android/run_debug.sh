#!/bin/bash
# usage: tools/android/run_debug.sh [build|emulator|install|run|shot|stop|all]...  (default: build)
# Debug APK without game data (ARCH_D_MOBILE §6.3 run 1, §6.4): cdylib for arm64-v8a + x86_64, NativeActivity
# manifest, aapt2 / zipalign / apksigner with a throwaway key in target/android/. Release APK with bundled data,
# the release keystore and the AAsset source are ANDROID2 (build_apk.sh), not here.
#   build     libth08.so for aarch64 + x86_64, pack + sign target/android/th08-debug.apk
#   emulator  start a headless emulator (own AVD th08dbg, no window, no audio) and wait for boot
#   install   adb install on the emulator only (EMU_SERIAL, never a physical device)
#   run       start the activity, wait, dump logcat to target/android/logcat.txt
#   shot      adb exec-out screencap → target/android/screen.png
#   stop      kill the emulator
# Env: ANDROID_SDK (default ~/.local/share/android-sdk/Sdk), ANDROID_API (min SDK / NDK clang API, default 26),
#      EMU_PORT (default 5584), PROFILE (release|dev, default release), TARGETS (default "aarch64 x86_64").
set -euo pipefail
cd "$(dirname "$0")/../.."
SDK=${ANDROID_SDK:-$HOME/.local/share/android-sdk/Sdk}
API=${ANDROID_API:-26}
PKG=org.th08.port.debug
OUT=target/android
EMU_PORT=${EMU_PORT:-5584}
EMU_SERIAL=emulator-$EMU_PORT
PROFILE=${PROFILE:-release}
TARGETS=${TARGETS:-aarch64 x86_64}
BT=$(ls -d "$SDK"/build-tools/*/ | sort -V | tail -1)
BT=${BT%/}
PLATFORM=$(ls -d "$SDK"/platforms/android-*/ | sort -V | tail -1)
PLATFORM=${PLATFORM%/}
NDK=${ANDROID_NDK_HOME:-$(ls -d "$SDK"/ndk/*/ | sort -V | tail -1)}
NDK=${NDK%/}
NBIN=$NDK/toolchains/llvm/prebuilt/linux-x86_64/bin
ADB=$SDK/platform-tools/adb
mkdir -p "$OUT"

abi_of() { case $1 in aarch64) echo arm64-v8a ;; x86_64) echo x86_64 ;; *) echo "unknown target $1" >&2; exit 1 ;; esac; }

build_lib() {
  local arch=$1 triple=$1-linux-android up
  up=$(tr 'a-z-' 'A-Z_' <<< "$triple")
  rustup target list --installed | grep -qx "$triple" || { echo "adding rustup target $triple"; rustup target add "$triple"; }
  env "CC_${triple//-/_}=$NBIN/$triple$API-clang" "CXX_${triple//-/_}=$NBIN/$triple$API-clang++" \
    "AR_${triple//-/_}=$NBIN/llvm-ar" "CARGO_TARGET_${up}_LINKER=$NBIN/$triple$API-clang" \
    ANDROID_NDK_HOME="$NDK" ANDROID_NDK_ROOT="$NDK" \
    cargo build --profile "$PROFILE" --lib -p th08 --target "$triple"
  local dir=$PROFILE; [[ $PROFILE == dev ]] && dir=debug
  mkdir -p "$OUT/stage/lib/$(abi_of "$arch")"
  cp "target/$triple/$dir/libth08.so" "$OUT/stage/lib/$(abi_of "$arch")/"
  # libc++_shared is not needed: liblzma is C, and the crate links no C++.
}

build() {
  rm -rf "$OUT/stage"
  for a in $TARGETS; do build_lib "$a"; done
  cat > "$OUT/AndroidManifest.xml" << EOF
<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="$PKG" android:versionCode="1" android:versionName="0.1.0">
  <uses-sdk android:minSdkVersion="$API" android:targetSdkVersion="${PLATFORM##*-}"/>
  <application android:label="th08 (debug)" android:hasCode="false" android:debuggable="true" android:extractNativeLibs="false">
    <activity android:name="android.app.NativeActivity" android:exported="true"
        android:configChanges="orientation|screenSize|screenLayout|keyboardHidden|keyboard|navigation|uiMode|density"
        android:screenOrientation="sensorLandscape" android:theme="@android:style/Theme.NoTitleBar.Fullscreen">
      <meta-data android:name="android.app.lib_name" android:value="th08"/>
      <intent-filter>
        <action android:name="android.intent.action.MAIN"/>
        <category android:name="android.intent.category.LAUNCHER"/>
      </intent-filter>
    </activity>
  </application>
</manifest>
EOF
  "$BT/aapt2" link -I "$PLATFORM/android.jar" --manifest "$OUT/AndroidManifest.xml" -o "$OUT/base.apk"
  # Native libs stored (extractNativeLibs=false needs them uncompressed and page-aligned).
  python3 -I -c 'import os,sys,zipfile
z=zipfile.ZipFile(sys.argv[1],"a",zipfile.ZIP_STORED)
for d,_,fs in os.walk(sys.argv[2]+"/lib"):
    for f in sorted(fs): p=os.path.join(d,f); z.write(p,os.path.relpath(p,sys.argv[2]))
z.close()' "$OUT/base.apk" "$OUT/stage"
  "$BT/zipalign" -P 16 -f 4 "$OUT/base.apk" "$OUT/aligned.apk"
  local ks=$OUT/debug.keystore
  [[ -f $ks ]] || keytool -genkeypair -keystore "$ks" -storepass android -keypass android -alias th08debug \
    -keyalg RSA -keysize 2048 -validity 10000 -dname "CN=th08 debug" > /dev/null 2>&1
  "$BT/apksigner" sign --ks "$ks" --ks-pass pass:android --key-pass pass:android \
    --out "$OUT/th08-debug.apk" "$OUT/aligned.apk"
  rm -f "$OUT/base.apk" "$OUT/aligned.apk" "$OUT/th08-debug.apk.idsig"
  ls -l "$OUT/th08-debug.apk"
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
  emu_adb shell am start -n "$PKG/android.app.NativeActivity"
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
