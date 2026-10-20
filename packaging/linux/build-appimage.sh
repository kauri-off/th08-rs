#!/usr/bin/env bash
# Builds th08-x86_64.AppImage from a release build (impl34). Usage: build-appimage.sh [out dir]
# Bundles only the port binary: glibc, libasound and the GPU/window-system libraries come from the host.
set -euo pipefail

root=$(cd "$(dirname "$0")/../.." && pwd)
out=$(mkdir -p "${1:-$root/target/appimage}" && cd "${1:-$root/target/appimage}" && pwd)
tools=$out/tools

appimagetool_url=https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage
appimagetool_sha=ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0
runtime_url=https://github.com/AppImage/type2-runtime/releases/download/20251108/runtime-x86_64
runtime_sha=2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d

fetch() { # url sha256 dest
    if ! echo "$2  $3" | sha256sum -c --status 2>/dev/null; then
        curl -sSfL "$1" -o "$3.part"
        echo "$2  $3.part" | sha256sum -c --quiet
        mv "$3.part" "$3"
    fi
}
mkdir -p "$tools"
fetch "$appimagetool_url" "$appimagetool_sha" "$tools/appimagetool"
fetch "$runtime_url" "$runtime_sha" "$tools/runtime-x86_64"
chmod +x "$tools/appimagetool"

cargo build --release --locked --bin th08 --manifest-path "$root/Cargo.toml"
bin=${CARGO_TARGET_DIR:-$root/target}/release/th08

appdir=$out/AppDir
rm -rf "$appdir"
install -Dm755 "$bin" "$appdir/usr/bin/th08"
strip "$appdir/usr/bin/th08"
install -Dm644 "$root/packaging/linux/th08.desktop" "$appdir/usr/share/applications/th08.desktop"
install -Dm644 "$root/packaging/linux/th08.png" "$appdir/usr/share/icons/hicolor/256x256/apps/th08.png"
ln -s usr/share/applications/th08.desktop "$appdir/th08.desktop"
ln -s usr/share/icons/hicolor/256x256/apps/th08.png "$appdir/th08.png"
ln -s th08.png "$appdir/.DirIcon"
ln -s usr/bin/th08 "$appdir/AppRun"

rm -f "$out/th08-x86_64.AppImage"
# Extract-and-run: no FUSE needed (CI containers). appimagetool can fail with exit 0, so check the output.
ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 "$tools/appimagetool" --no-appstream \
    --runtime-file "$tools/runtime-x86_64" "$appdir" "$out/th08-x86_64.AppImage"
test -s "$out/th08-x86_64.AppImage"
echo "built $out/th08-x86_64.AppImage"
