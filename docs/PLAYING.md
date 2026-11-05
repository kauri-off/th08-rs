# Playing th08

The port contains no game data. You need your own copy of 東方永夜抄 ~ Imperishable Night; the port reads its data
files and nothing else.

## Downloads

Every [release](https://github.com/kauri-off/th08-rs/releases) has three files:

| Platform | File | How to start |
|---|---|---|
| Windows 10 / 11 (x64) | `th08-<version>-windows-exe.exe` | a single portable exe; run it from any folder |
| Linux (x86-64, glibc 2.35+: Ubuntu 22.04+, Debian 12+, Fedora 36+, SteamOS 3) | `th08-<version>-linux-appimage.AppImage` | `chmod +x` it, then run it |
| Android 8.0+ (arm64) | `th08-<version>-android-apk.apk` | install it (allow installs from your browser or file manager when asked) |

The browser build at https://kauri-off.github.io/th08-rs/ needs no download; you select the game files in the page.

## Game files

From your Imperishable Night folder take:

- `th08.dat`: the game itself (required);
- `thbgm.dat`: the music.

If you have repacked files (`th08.pak`, `thbgm.pak`, see [arch/ASSETS_FORMAT.md](arch/ASSETS_FORMAT.md)), they work
in place of the `.dat` files. File names are matched in any letter case.

## Where to put them

### Windows and Linux

Nothing to copy. On the first start the launcher asks for the game folder: pick the folder that contains `th08.dat`
(the **Browse…** button in the launcher's game folder row changes it later). The choice is saved in `port.cfg`.
`th08 "<game folder>"` on the command line or the `TH08_DIR` environment variable work too.

### Android

Today the app reads the game files from its own folder on the shared storage:

```
/storage/emulated/0/Android/data/org.th08.port/files/game/
```

The app creates this folder on its first start. Copy `th08.dat` and `thbgm.dat` into it, then tap **Check again**
in the launcher's Game data row. Two ways:

- **From a computer** (USB debugging on):
  `adb push th08.dat thbgm.dat /storage/emulated/0/Android/data/org.th08.port/files/game/`
- **With a file manager on the phone:** many file managers on Android 11 and newer cannot open `Android/data`; use
  one that can, or adb.

The launcher's Game data row shows the exact folder and whether the files were found.

## Saves, replays and settings

| | Windows | Linux | Android |
|---|---|---|---|
| `port.cfg` (launcher settings) | `%APPDATA%\th08\` | `~/.config/th08/` (`$XDG_CONFIG_HOME/th08/`) | `/data/user/0/org.th08.port/files/` |
| `score.dat`, `th08.cfg` | same folder | same folder | same folder |
| replays (`replay/`) | `%APPDATA%\th08\replay\` | `~/.config/th08/replay/` | `/data/user/0/org.th08.port/files/replay/` |
| downloaded translations (`packs/`) | `%APPDATA%\th08\packs\` | `~/.config/th08/packs/` | `/data/user/0/org.th08.port/files/packs/` |

- Desktop: the `save_dir` setting (`th08 --help`) moves `score.dat`, `th08.cfg` and the replays elsewhere.
- Android: this is the app's private storage. Other apps and a file manager cannot see it, and **uninstalling the
  app deletes it**, saves included.
- Browser: everything is kept in the site's local storage.
- With any cheat setting on, `score.dat` and replays are not written (the launcher says so).

## Translations

The game text is the original Japanese by default. To get another language, open the **Translations** panel in the
launcher's Language section and download one: the port fetches the translation from Touhou Patch Center
(thpatch.net) and builds the pack on your device from your own game data, so the game folder must be set first.
Then pick it in **Game text language** (or leave it at "Same as the screens"). Installed packs can be updated and
removed in the same panel. The browser build cannot download translations yet.

The launcher and the in-game port menu themselves come in Japanese, English and Russian (**Language** setting).

## Controls

- Keyboard: as in the original. Arrow keys move, `Z` shoots, `X` bombs, `Shift` focuses, `Esc` pauses. `F1` opens
  the port menu.
- Gamepads work on Windows, Linux and Android; on Android the pad's Select button opens the port menu.
- Android touch: an on-screen overlay. Its **MENU** button (or Back) opens the port menu, where **Touch controls
  layout** lets you move, resize, hide and fade each control, separately for landscape and portrait.

## Android channels

Releases carry `org.th08.port` ("th08"); `org.th08.port.dev` ("th08 dev") and `org.th08.port.debug` ("th08 debug")
are local builds (see [CONTRIBUTING.md](../CONTRIBUTING.md#packaging-and-versions)). The three install side by side
and each has its own game folder and saves: replace `org.th08.port` in the paths above with the package name.

## Legal

This is an unofficial fan project, not affiliated with Team Shanghai Alice. It contains no game data; play it only
with a copy of Touhou 8 you own. Translations are by Touhou Patch Center and its contributors (CC BY-SA 4.0).
