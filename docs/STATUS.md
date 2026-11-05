# Status

## What works

- The whole game: all stages including Extra, all 12 shot types, bombs and Last Spells, spell cards and spell
  practice, dialogue, the night clock, menus and options, key config, replays (record and play), `score.dat`,
  result screen, endings and staff roll, the music room, BGM (WAV from `thbgm.dat` or MIDI) and sound effects.
- Game logic is bit-exact with the original 1.00d: every recorded session (golden, fullrun_noclip, demos) and the
  third-party replay sets (ref500 490/490, ref_b2 498/498) produce identical per-frame traces, RNG calls included.
- Drawing is visually identical; every remaining draw-log difference is in an accepted class
  ([parity/README.md](parity/README.md)).
- Platforms: Linux and Windows (native), the browser (WebAssembly, WebGPU or WebGL2), Android (APK).
- Releases on GitHub (Windows exe, Linux AppImage, Android APK, no game data), built and published by a manual
  workflow; player guide in [PLAYING.md](PLAYING.md).
- Android channels side by side: `release` (`org.th08.port`, GitHub), `dev` and `debug` (local builds).
- Input: keyboard, gamepads (Windows, Linux, Android), an Android touch overlay with a layout editor in the port menu
  (move, resize, opacity, show / hide; landscape and portrait).
- Launcher with every port setting (egui on desktop and web, Compose on Android); launcher text in Japanese, English
  and Russian.
- Translated game text: the launcher's Translations panel downloads a Touhou Patch Center translation and builds a
  presentation pack on the device from the player's game data (desktop and Android; [arch/I18N.md](arch/I18N.md)).
- Optional repacked game data (`.pak`, Vorbis BGM) for smaller local packages
  ([arch/ASSETS_FORMAT.md](arch/ASSETS_FORMAT.md)).
- The idiomatic rewrite is done up to and including the module pass: one cloneable `World`, drawing as a command
  list, a core crate with no window, audio or file system access ([arch/ARCH_SUMMARY.md](arch/ARCH_SUMMARY.md)).

## Known differences from the original

- Dialogue and spell-card text uses a system Japanese font (native) or the browser's font (web): the original's GDI
  MS Gothic pixels cannot be reproduced (the only `SPEC-GAP` left in the code).
- Draw-log-only differences that are invisible on screen and accepted ([PARITY_POLICY.md](PARITY_POLICY.md)):
  sub-pixel float rounding of orbit stars, particles and glyph snapping; the frame on which a loader finishes; the
  host FPS digits; the HUD repaints the whole side panel instead of only the changed rows.
- Port-only additions (launcher, port settings, port menu, touch overlay, translations) are not part of the original.

## Limits

- Android: the game files go into the app's external files folder by file manager or adb; no file picker yet.
- The browser build cannot download translations.

## Next

- Android: a file-picker button for the game files.
- Translations: per-screen layout tuning of translated text ([arch/I18N.md](arch/I18N.md)).
- Mobile web polish.
- Optional renderer rewrite (the last step of the rewrite plan), checked by image comparison.
