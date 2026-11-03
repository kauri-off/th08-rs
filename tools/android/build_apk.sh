#!/bin/bash
# usage: tools/android/build_apk.sh [--abi ABI]... [--data DIR] (--keystore PROPS | --debug-key) [--out APK]
# Release APK org.th08.port (ARCH_D_MOBILE §2.1, §6.3 run 2): NativeActivity, libth08.so per ABI, optional game data
# as stored assets, zipalign + apksigner. Commits no game data; outputs under target/android/.
#   --abi ABI        arm64-v8a (default) or x86_64; repeat for both
#   --data DIR       add DIR's th08.dat and thbgm.dat (any letter case) as stored assets/th08.dat, assets/thbgm.dat
#   --keystore PROPS keystore.properties (storeFile / storePassword / keyAlias / keyPassword; storeFile relative to
#                    the file); passwords reach apksigner only via --ks-pass env:, never argv, logs or files
#   --debug-key      sign with run_debug.sh's throwaway key target/android/debug.keystore instead
#   --out APK        output path (default target/android/th08-release.apk)
# Env: ANDROID_SDK, ANDROID_API (min SDK / NDK clang API, default 26), PROFILE (default release), VERSION_CODE (1).
# cargo runs under flock target/.agents.lock inside the script, so do not wrap the script itself in that flock.
set -euo pipefail
cd "$(dirname "$0")/../.."
SDK=${ANDROID_SDK:-$HOME/.local/share/android-sdk/Sdk}
API=${ANDROID_API:-26}
PROFILE=${PROFILE:-release}
PKG=org.th08.port
OUT=target/android
abis=() data= props= debug=0 apk=$OUT/th08-release.apk
while [[ $# -gt 0 ]]; do
  case $1 in
    --abi) abis+=("$2"); shift 2 ;;
    --data) data=$2; shift 2 ;;
    --keystore) props=$2; shift 2 ;;
    --debug-key) debug=1; shift ;;
    --out) apk=$2; shift 2 ;;
    -h|--help) sed -n '2,13p' "$0"; exit 0 ;;
    *) echo "unknown option $1" >&2; exit 1 ;;
  esac
done
[[ ${#abis[@]} -gt 0 ]] || abis=(arm64-v8a)
[[ -n $props || $debug == 1 ]] || { echo "need --keystore PROPS or --debug-key" >&2; exit 1; }
[[ -z $props || -f $props ]] || { echo "no keystore properties file $props" >&2; exit 1; }

BT=$(ls -d "$SDK"/build-tools/*/ | sort -V | tail -1); BT=${BT%/}
PLATFORM=$(ls -d "$SDK"/platforms/android-*/ | sort -V | tail -1); PLATFORM=${PLATFORM%/}
NDK=${ANDROID_NDK_HOME:-$(ls -d "$SDK"/ndk/*/ | sort -V | tail -1)}; NDK=${NDK%/}
NBIN=$NDK/toolchains/llvm/prebuilt/linux-x86_64/bin
LOCK=$PWD/target/.agents.lock
stage=$OUT/release-stage
mkdir -p "$OUT" "$(dirname "$apk")"
rm -rf "$stage"

arch_of() { case $1 in arm64-v8a) echo aarch64 ;; x86_64) echo x86_64 ;; *) echo "unknown ABI $1" >&2; exit 1 ;; esac; }

for abi in "${abis[@]}"; do
  triple=$(arch_of "$abi")-linux-android
  up=$(tr 'a-z-' 'A-Z_' <<< "$triple")
  rustup target list --installed | grep -qx "$triple" || rustup target add "$triple"
  env "CC_${triple//-/_}=$NBIN/$triple$API-clang" "CXX_${triple//-/_}=$NBIN/$triple$API-clang++" \
    "AR_${triple//-/_}=$NBIN/llvm-ar" "CARGO_TARGET_${up}_LINKER=$NBIN/$triple$API-clang" \
    ANDROID_NDK_HOME="$NDK" ANDROID_NDK_ROOT="$NDK" \
    flock "$LOCK" cargo build --profile "$PROFILE" --lib -p th08 --target "$triple"
  dir=$PROFILE; [[ $PROFILE == dev ]] && dir=debug
  mkdir -p "$stage/lib/$abi"
  cp "target/$triple/$dir/libth08.so" "$stage/lib/$abi/"
  "$NBIN/llvm-strip" --strip-debug "$stage/lib/$abi/libth08.so"
done

cat > "$stage/AndroidManifest.xml" << EOF
<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="$PKG" android:versionCode="${VERSION_CODE:-1}" android:versionName="0.1.0">
  <uses-sdk android:minSdkVersion="$API" android:targetSdkVersion="${PLATFORM##*-}"/>
  <application android:label="th08" android:hasCode="false" android:extractNativeLibs="false">
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
"$BT/aapt2" link -I "$PLATFORM/android.jar" --manifest "$stage/AndroidManifest.xml" -o "$stage/base.apk"

# Libs and game data stored: extractNativeLibs=false needs uncompressed .so, AAsset fds need uncompressed assets
# (ARCH_D §2.1). The game files are streamed into the zip from DIR, not copied.
assets=()
if [[ -n $data ]]; then
  for f in th08.dat thbgm.dat; do
    src=$(find "$data" -maxdepth 1 -type f -iname "$f" -print -quit)
    [[ -n $src ]] || { echo "no $f in $data" >&2; exit 1; }
    assets+=("$src")
  done
fi
python3 -I -c 'import os,sys,zipfile
z=zipfile.ZipFile(sys.argv[1],"a",zipfile.ZIP_STORED,allowZip64=True)
for d,_,fs in os.walk(sys.argv[2]+"/lib"):
    for f in sorted(fs): p=os.path.join(d,f); z.write(p,os.path.relpath(p,sys.argv[2]))
for p in sys.argv[3:]: z.write(p,"assets/"+os.path.basename(p).lower())
z.close()' "$stage/base.apk" "$stage" "${assets[@]}"
"$BT/zipalign" -P 16 -f 4 "$stage/base.apk" "$stage/aligned.apk"
rm -f "$stage/base.apk"

if [[ $debug == 1 ]]; then
  ks=$OUT/debug.keystore
  [[ -f $ks ]] || keytool -genkeypair -keystore "$ks" -storepass android -keypass android -alias th08debug \
    -keyalg RSA -keysize 2048 -validity 10000 -dname "CN=th08 debug" > /dev/null 2>&1
  TH08_KS_PASS=android TH08_KEY_PASS=android "$BT/apksigner" sign --ks "$ks" --ks-key-alias th08debug \
    --ks-pass env:TH08_KS_PASS --key-pass env:TH08_KEY_PASS --out "$apk" "$stage/aligned.apk"
else
  # Values are read here and handed to apksigner through its environment only.
  prop() { sed -n "s/^[[:space:]]*$1[[:space:]]*[=:][[:space:]]*//p" "$props" | tail -1 | tr -d '\r'; }
  ks=$(prop storeFile)
  [[ $ks == /* ]] || ks=$(dirname "$props")/$ks
  [[ -f $ks ]] || { echo "keystore named by $props not found" >&2; exit 1; }
  alias=$(prop keyAlias)
  TH08_KS_PASS=$(prop storePassword) TH08_KEY_PASS=$(prop keyPassword) "$BT/apksigner" sign --ks "$ks" \
    --ks-key-alias "$alias" --ks-pass env:TH08_KS_PASS --key-pass env:TH08_KEY_PASS --out "$apk" \
    "$stage/aligned.apk"
fi
rm -rf "$stage" "$apk.idsig"
"$BT/apksigner" verify "$apk"
ls -l "$apk"
