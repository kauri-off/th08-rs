#!/bin/bash
# usage: tools/android/build_apk.sh [--abi ABI]... [--data DIR] (--keystore PROPS | --debug-key) [--variant V] [--out APK]
# APK org.th08.port through Gradle (android/, MATERIAL1): Compose launcher + NativeActivity game in ":game", libth08.so
# per ABI (cargo + NDK clang, tools/android/cargo_android.sh), the settings schema, optional stored game data.
#   --abi ABI        arm64-v8a (default) or x86_64; repeat for both
#   --data DIR       add DIR's th08.dat and thbgm.dat (any letter case) as stored assets/th08.dat, assets/thbgm.dat
#   --keystore PROPS keystore.properties (storeFile / storePassword / keyAlias / keyPassword; storeFile relative to
#                    the file); read only inside the Gradle build, never printed or copied
#   --debug-key      sign with the throwaway key target/android/debug.keystore instead
#   --variant V      release (default) or debug (org.th08.port.debug, debuggable; run_debug.sh)
#   --out APK        output path (default target/android/th08-release.apk)
# Env: ANDROID_SDK, ANDROID_API (min SDK / NDK clang API, default 26), PROFILE (cargo profile, default release),
#      VERSION_CODE (1). cargo runs under flock target/.agents.lock inside the build, so do not wrap this script in it.
set -euo pipefail
cd "$(dirname "$0")/../.."
SDK=${ANDROID_SDK:-$HOME/.local/share/android-sdk/Sdk}
OUT=target/android
abis=() data= props= debug=0 variant=release apk=
while [[ $# -gt 0 ]]; do
  case $1 in
    --abi) abis+=("$2"); shift 2 ;;
    --data) data=$2; shift 2 ;;
    --keystore) props=$2; shift 2 ;;
    --debug-key) debug=1; shift ;;
    --variant) variant=$2; shift 2 ;;
    --out) apk=$2; shift 2 ;;
    -h|--help) sed -n '2,13p' "$0"; exit 0 ;;
    *) echo "unknown option $1" >&2; exit 1 ;;
  esac
done
[[ ${#abis[@]} -gt 0 ]] || abis=(arm64-v8a)
[[ $variant == release || $variant == debug ]] || { echo "unknown variant $variant" >&2; exit 1; }
[[ -n $apk ]] || apk=$OUT/th08-$variant.apk
[[ -n $props || $debug == 1 ]] || { echo "need --keystore PROPS or --debug-key" >&2; exit 1; }
[[ -z $props || -f $props ]] || { echo "no keystore properties file $props" >&2; exit 1; }
[[ -z $data || -d $data ]] || { echo "no folder $data" >&2; exit 1; }
mkdir -p "$OUT" "$(dirname "$apk")"

args=(-Pth08.abis="$(IFS=,; echo "${abis[*]}")" -Pth08.minSdk="${ANDROID_API:-26}"
  -Pth08.profile="${PROFILE:-release}" -Pth08.versionCode="${VERSION_CODE:-1}")
[[ -n $data ]] && args+=(-Pth08.data="$(realpath "$data")")
[[ -n $props ]] && args+=(-Pth08.keystore="$(realpath "$props")")
task=assemble${variant^}
ANDROID_HOME=$SDK gradle --no-daemon --console=plain -q -p android "$task" "${args[@]}"

BT=$(ls -d "$SDK"/build-tools/*/ | sort -V | tail -1); BT=${BT%/}
cp "android/app/build/outputs/apk/$variant/app-$variant.apk" "$apk"
"$BT/apksigner" verify "$apk"
ls -l "$apk"
