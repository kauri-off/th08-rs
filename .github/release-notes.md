<!-- Release notes template: the release workflow publishes it above the generated commit list. Fill in the
highlights afterwards with `gh release edit vX.Y.Z --notes-file …`. -->

## Highlights

See the changes listed below.

## Install

The port contains no game data: you need your own copy of Touhou 8 ~ Imperishable Night (`th08.dat`, `thbgm.dat`).

| Platform | File |
|---|---|
| Windows 10 / 11 (x64) | `th08-<version>-windows-exe.exe`: portable, run it from any folder |
| Linux (x86-64, glibc 2.35+) | `th08-<version>-linux-appimage.AppImage`: `chmod +x`, then run |
| Android 8.0+ (arm64) | `th08-<version>-android-apk.apk`: installs as `org.th08.port`, updates the previous release |

- Windows / Linux: pick your game folder in the launcher on the first start.
- Android: in the launcher, tap **Import game files** and pick `th08.dat` and `thbgm.dat`.

Saves, translations and the rest: [docs/PLAYING.md](https://github.com/kauri-off/th08-rs/blob/master/docs/PLAYING.md).

## Known issues

- Dialogue and spell-card text uses a system Japanese font, not the original's MS Gothic pixels.
- Uninstalling the Android app deletes its saves and replays.
- The browser build cannot download translations.
