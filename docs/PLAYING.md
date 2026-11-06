# Playing th08

The port contains no game data. You need your own copy of 東方永夜抄 ~ Imperishable Night; the port reads its data
files and nothing else.

## Downloads

Every [release](https://github.com/kauri-off/th08-rs/releases) has these files:

| Platform | File | How to start |
|---|---|---|
| Windows 10 / 11 (x64), recommended | `th08-<version>-windows-nsis.exe` | the installer: installs for your user only (no admin) into `%LOCALAPPDATA%\Programs\th08`, with Start menu and desktop shortcuts and an uninstaller |
| Windows 10 / 11 (x64), portable | `th08-<version>-windows-exe.exe` | a single portable exe; run it from any folder |
| Linux (x86-64, glibc 2.35+: Ubuntu 22.04+, Debian 12+, Fedora 36+, SteamOS 3) | `th08-<version>-linux-appimage.AppImage` | `chmod +x` it, then run it; it can update itself with [AppImageUpdate](https://github.com/AppImageCommunity/AppImageUpdate) or an AppImage manager (Gear Lever, AppImageLauncher, AM), which use the `.zsync` file next to it in the release |
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

In the launcher's Game data row, tap **Import game files** and pick `th08.dat` and `thbgm.dat` (or `.pak` files)
wherever they are: Downloads, a USB drive, cloud storage. The app checks them, converts them into its own format
and keeps them in its private storage; the picked files are not changed and can be deleted afterwards. Import needs
0.1.2 or newer.

## Saves, replays and settings

| | Windows | Linux | Android |
|---|---|---|---|
| `port.cfg` (launcher settings) | `%APPDATA%\th08\` | `~/.config/th08/` (`$XDG_CONFIG_HOME/th08/`) | `/data/user/0/org.th08.port/files/` |
| `score.dat`, `th08.cfg` | same folder | same folder | same folder |
| replays (`replay/`) | `%APPDATA%\th08\replay\` | `~/.config/th08/replay/` | `/data/user/0/org.th08.port/files/replay/` |
| downloaded translations (`packs/`) | `%APPDATA%\th08\packs\` | `~/.config/th08/packs/` | `/data/user/0/org.th08.port/files/packs/` |

- Desktop: the `save_dir` setting (`th08 --help`) moves `score.dat`, `th08.cfg` and the replays elsewhere.
- Android: this is the app's private storage. Other apps and a file manager cannot see it, and **uninstalling the
  app deletes it**, saves included. Before you uninstall, switch to another build (a different signing key, an older
  version) or move to a new phone, use **Export saves** in the launcher's Saves section and keep the zip; **Import
  saves** puts it back.
- Android Auto Backup (when it is on in the phone's settings) backs up `score.dat`, `th08.cfg`, `port.cfg` and
  `replay/` only, never the game files or translations, and restores them when the same app is installed again.
  It does not carry saves to a build signed with another key; export them for that. On Android 10 and newer the
  uninstall dialog can also offer to keep the app's data.
- Browser: everything is kept in the site's storage (saves and settings in local storage, game files and downloaded
  translations as `packs/lang_<code>.zip` in the origin private file system); clearing the site data deletes them.
  The page asks the browser to keep this storage when you pick the game files, but a browser may still clear it
  (Safari after a week without a visit), so export the saves from time to time.
- With any cheat setting on, `score.dat` and replays are not written (the launcher says so).

## Translations

The game text is the original Japanese by default. To get another language, open the **Translations** panel in the
launcher's Language section and download one: the port fetches the translation from Touhou Patch Center
(thpatch.net) and builds the pack on your device from your own game data, so the game folder must be set first.
Then pick it in **Game text language** (or leave it at "Same as the screens"). Installed packs can be updated and
removed in the same panel. The browser build does the same with the game files selected in the page and keeps the
packs in the site's storage.

The launcher and the in-game port menu themselves come in Japanese, English and Russian (**Language** setting).

## Updates

The launcher of a release build (the files on GitHub, Android `org.th08.port`) asks GitHub's release list at most
once a day whether a newer version is out, and only while the launcher screen is open, never during play. When there
is one, a row at the top says "Version X is available" with **What is new** (the release page) and **Download** (the
file for your system: the APK on Android, the installer or the portable exe on Windows, matching the one you run, the
AppImage on Linux). Offline or on any error it shows nothing. The request carries no data about you or your game; the
time of the last check is kept in `update_check.json` next to `port.cfg`. Turn it off with **Check for updates** in
the Display section (`update_check = off` in `port.cfg`). Besides translation downloads it is the only network
request the port makes. Local builds and the browser build never check.

On Android the release app (`org.th08.port`) also offers **Update**, which installs the new version from the
launcher itself:

- It downloads the release's APK into the app's cache (with progress and **Cancel**) and the release's `SHA256SUMS`,
  and refuses the file unless its SHA-256 matches. It then checks that the APK is this app, has a higher version and
  is **signed with the same key** as the installed one; anything else is refused and deleted. A failed, cancelled or
  interrupted download is deleted, at the latest on the next start.
- The first time, Android asks you to allow installs from th08: **Open settings**, turn on "Allow from this source",
  go back, and the install goes on. Android then asks you to confirm the update; the app closes when it is in, and
  the launcher says "Updated to version X" when you open it again.
- It is an in-place update: saves (`score.dat`, replays), settings, the imported game files and translations stay.
- Any failure shows the reason and the **Download** button, which opens the APK in your browser as before. It works
  only from the launcher screen, never while the game is running.

On Windows the installed copy (the one the installer put in place, as recorded in the uninstall entry) also offers
**Update**:

- It downloads the release's installer into the temp folder (with progress and **Cancel**) and the release's
  `SHA256SUMS`, and refuses the file unless its SHA-256 matches. It then starts the installer silently in update mode
  (`/S /UPDATE`) and closes the launcher.
- The installer waits for th08 to close (up to 30 seconds; after that it asks you to close the game and press
  **Retry**, or **Cancel** to leave the old version as it is), replaces the program files, and starts the game again.
  Saves, `port.cfg` and translations in `%APPDATA%\th08` stay as they are, and so do your shortcuts.
- Any failure shows the reason, and **Download** still opens the installer in your browser. The portable exe has no
  in-app update; it keeps the **Download** link.
- The same update by hand: `th08-<version>-windows-nsis.exe /S /UPDATE` (without `/UPDATE` it does not restart the
  game and re-creates the shortcuts).

On Android, [Obtainium](https://github.com/ImranR98/Obtainium) can track the GitHub releases and install new APKs;
the Linux AppImage updates itself with AppImageUpdate (see Downloads).

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
