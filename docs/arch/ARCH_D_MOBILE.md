# Architecture proposal D — mobile version

Brief: docs/briefs/ARCH_D_MOBILE_BRIEF.md. Fits into REWRITE_PLAN.md (step 7 `Host` in progress) and the day-0 table
in ARCH_SUMMARY.md. Measured on the working tree of 2026-11-01 (IMPL158 mid-edit). Scratch: target/arch_D/.

**Scope (user decision 2026-11-01):** targets are **native Android (APK)** and **mobile web** (existing wasm build).
**iOS is not supported.** Desktop packaging (NSIS, AppImage) is designed elsewhere and is out of scope here.
Android APKs are built locally **with the user's th08.dat / thbgm.dat bundled inside**, for personal use, never
distributed; user-provided files are the secondary route.

## 0. Measurements

| what | result |
|---|---|
| Toolchain | rustc 1.98.0; `rustup target add aarch64-linux-android` done (no other rustup change) |
| Android SDK | `~/.local/share/android-sdk/Sdk`: NDK 27.1.12297006, build-tools 35/36 (aapt2, zipalign, apksigner, d8), platform android-36, an x86_64 system image + AVD, `~/.android/debug.keystore`; `adb`, `scrcpy`, `gradle`, JDK 17 installed. `ANDROID_HOME` is **not** set |
| `xbuild` on PATH | `/usr/bin/xbuild` is **Mono's XBuild**, not rust-mobile xbuild. `cargo-apk`/`cargo-ndk` not installed. Packaging below therefore uses aapt2 directly |
| `th08-formats`, `th08-core` for aarch64-linux-android | `cargo check` clean |
| `th08` lib for aarch64-linux-android, tree as-is | fails in `android-activity 0.6.1`: winit needs feature `android-native-activity` or `android-game-activity` |
| same, scratch copy with that winit feature + `rfd` cfg'd off Android | **all dependencies compile**: wgpu 30 (Vulkan/GLES), cpal 0.18.2 (AAudio via `ndk`), egui/egui-wgpu/egui-winit 0.37 (arboard is already cfg'd off Android by egui-winit), gilrs 0.11, liblzma `static` (C built by NDK clang via `CC_aarch64_linux_android`), rustysynth, ab_glyph, chrono, directories. Remaining errors: 5 × `rfd::` in `src/launcher.rs` (46, 47, 53, 60, 397) — the only Android-specific blocker — plus 4 errors that the host build has too (IMPL158 in progress: `HostIo`, `load_score`) |
| gilrs on Android | `gilrs-core 0.6.8` has no Android backend (platform dirs: linux, macos, windows, wasm, default=dummy) → **no gamepads via gilrs** |
| winit 0.30.13 Android gamepad | buttons arrive as `KeyboardInput` with `PhysicalKey::Unidentified(NativeKeyCode::Android(code))` (A/B/X/Y/L1/R1/Start…); D-pad keycodes map to `ArrowUp…` (keycodes.rs:69); **stick/hat axes are not exposed** (every MotionEvent becomes a `Touch`, mod.rs:376) |
| winit Android lifecycle | `Resumed`/`Suspended` per native window (mod.rs:215/218), `Focused`, `LowMemory`; `content_rect()` for insets; `android_app().internal_data_path()/external_data_path()/asset_manager()` available |
| `directories::BaseDirs` on Android | `dirs-sys 0.5` `home_dir()` = `$HOME` else `None` on Android; NativeActivity has no `$HOME` → **settings (settings.rs:1236) and soundfont search (midi.rs:440) silently get no directory** |
| Game data (user's copy) | `th08.dat` **46 838 025 B** (xz -6: 40.2 MB, −14 %); `thbgm.dat` **449 961 024 B** (raw 44.1 kHz s16 stereo; xz -6: 423.5 MB, −6 %). Total ≈ 497 MB, essentially incompressible |
| Web build | web/dist: `th08_bg.wasm` 11.1 MB, `th08.js` 162 KB; page already caches the .dat files in OPFS (web/index.html) |
| Music memory | `audio.rs:438` reads a whole track via `read_range` and converts to `[f32;2]` → 8 B per sample frame (≈ 21 MB per audio minute) |
| Pacing loop | app.rs RedrawRequested: early redraws sleep ≤ 1 ms and request another redraw → ~1 kHz wake-ups between logic frames |
| Font | text.rs font list has no Android path; Android ships `/system/fonts/NotoSansCJK-Regular.ttc` |
| Score save | `Ctx::exit_save` → `game.save_score(&mut self.rng, …)`: **writing score.dat consumes the game RNG** |

## 1. Targets and order

| route | stack | cost | notes |
|---|---|---|---|
| **Android native (APK)** | winit 0.30 `android-native-activity` + wgpu (Vulkan, GLES fallback) + cpal AAudio; `cdylib` `libth08.so` loaded by the system `NativeActivity` (`android:hasCode="false"`, no Java/Kotlin) | ≈ 6 IMPL runs incl. shared overlay (§6.3) | primary target; bundled data; best latency and battery; all deps already compile |
| **Mobile web** | existing wasm build + PWA manifest, fullscreen, orientation lock, audio unlock on touch, the same touch overlay | ≈ 2 runs on top of the overlay | zero install; data via file picker (+ OPFS cache, exists); no bundling (a public page must never serve the files) |
| iOS | — | — | not supported |

NativeActivity vs GameActivity: NativeActivity needs no Java and no Gradle, which keeps the build a shell script
(§2.1). GameActivity (needs Gradle + AndroidX) is only worth it for SAF folder picking and Paddleboat gamepad axes;
both have cheaper alternatives here (§2.3, §3.4).

**Order.**
1. Now, inside the rewrite: the day-0 items of §6.1 (mostly API shape), and add `cargo check --target
   aarch64-linux-android -p th08` to the **W** check (needs only the winit feature on Android + `rfd` cfg; ≈ 1 small
   deliverable) so the rewrite cannot regress portability.
2. After step 10 (crate cut; app churn in steps 7–10 would otherwise conflict): Android APK with bundled data,
   keyboard/gamepad only → lifecycle → touch overlay (shared code) → pacing/battery.
3. Mobile web polish (PWA, orientation, audio unlock) reusing the overlay.

## 2. Game data

### 2.1 Bundled in the APK (primary)

**Recommended: APK `assets/` stored uncompressed, read in place through `AAssetManager`.**
- Build: `aapt2 link -I android-36/android.jar --manifest AndroidManifest.xml -A <staging>/assets -0 .dat -o
  base.apk`, add `lib/arm64-v8a/libth08.so` (zip -0 is fine), `zipalign -p -f 16` (`-p` page-aligns .so; 16 KB pages
  on Android 15+), `apksigner sign --ks ~/.android/debug.keystore`. All tools are in build-tools 36. A ≈ 40-line
  `tools/android/build_apk.sh` (port-only, no spec) taking `--data <dir with th08.dat, thbgm.dat>`; staging and
  output under `target/android/`, never in git, never in `packaging/`.
- Read path: `AAssetManager_open(name, AASSET_MODE_RANDOM)` → `AAsset_openFileDescriptor64` gives `(fd, start, len)`
  because the entry is stored (`-0 .dat`); range reads are `pread(fd, start + off)`. No copy, no first-run delay,
  no second 497 MB on disk. th08.dat (47 MB) is read whole at load as today; thbgm.dat track reads stay ranged.
- Size: APK ≈ 497 MB data + release `.so` (to be measured once the tree builds; expect 10–20 MB). Sideload via
  `adb install` has no size limit (Play's 200 MB limit is irrelevant); install needs ≈ 1 GB free transiently.
- Compression buys nothing (−6 % on thbgm.dat) and would force AAsset to inflate on every seek; keep both stored.

**Fallback: extract to internal storage on first run** (`internal_data_path()/data/`). Advantage: the existing
directory source (`Source::Native` / `find_file`) works unchanged. Cost: 497 MB extra on disk for the life of the
install (APK stays in /data/app), 2–5 s first-run copy that must run off the main thread (ANR at 5 s) with a progress
screen. Use only if the AAsset fd path misbehaves on some device.

Not pursued: transcoding thbgm.dat to FLAC/Opus at build time (needs a decoder and changes the audio path; 450 MB is
acceptable for a personal sideload).

### 2.2 User-provided files (secondary)

- **Android, cheapest:** `adb push th08.dat thbgm.dat /sdcard/Android/data/<pkg>/files/` — the app reads
  `external_data_path()` with no permission and no Java; it is a plain directory source. The app checks there when
  the APK has no bundled assets (the same `.so` serves both builds).
- **Android SAF picker** (`ACTION_OPEN_DOCUMENT_TREE`, copy into app storage): needs an Activity result callback →
  Java subclass or GameActivity + Gradle. Defer; only if the APK is ever built without data for someone else's use.
- **Web:** unchanged: `<input type=file webkitdirectory>` or multi-file → OPFS cache (index.html) → `File.slice`
  range reads (assets.rs). Add `navigator.storage.persist()` so the 497 MB OPFS copy is not evicted.

### 2.3 How Host / the content layer expose it

Today `assets::Source` is a cfg'd enum (`Native(PathBuf)` | `Web(Rc<Vec<File>>)`). Make it an open set before step
8b fixes the content layer:

```rust
// App side (not core): where th08.dat / thbgm.dat bytes come from.
pub trait DataSource {
    fn exists(&self, name: &str) -> bool;
    async fn read(&self, name: &str) -> Result<Vec<u8>>;
    async fn read_range(&self, name: &str, off: u64, len: usize) -> Result<Vec<u8>>;
}
// impls: DirSource (desktop, Android external dir, extract fallback), ApkAssetSource (fd + start), WebFileSource
```

The archive parser and B's VFS sit on top of a `DataSource`; core sees only the `LogicView` built from it (decision
6). Saves are separate: `HostIo`/`Storage` gets its directory from the entry point (`PlatformDirs { config, saves }`):
desktop `directories`, Android `internal_data_path()`, web localStorage. No code path below the app may call
`directories::BaseDirs` (settings.rs:1236, midi.rs:440 do today).

## 3. Input

Everything ends as the original input word in `Host::poll_input` → `RawInput { keys: [u16; 2], pads }` (host.rs).
Logic, replays and sessions never see touches: a replay records the word, so a touch-played replay is a normal
replay and plays back on desktop.

### 3.1 Sources merged into the word
App-side `InputMux` ORs, per poll, the bits of: keyboard (`platform::Keyboard`), gamepad buttons (Android keycodes,
§3.4), and the touch overlay. ORing is what Input_Poll does with keyboard + joystick (15 §1.1). The joystick path
that needs cfg + ShotSlow stays in core via `pads`; Android gamepads go through `keys` bits, because the raw pad
state model (gilrs axes) has no Android backend. Focus loss (`Focused(false)`, e.g. notification shade) already
zeroes the keyboard word; the mux applies it to touch too.

### 3.2 Movement: relative drag (default) and virtual stick
- **Relative drag (recommended for a bullet hell):** a finger anywhere in the movement zone; on touch-down the
  current player position is the anchor, the target is anchor + finger delta × sensitivity (port setting, default
  1.0 in game pixels). Each poll emits the direction bits that move the player towards the target, with a dead zone
  of one frame's step (per-axis, so diagonals come out naturally) to avoid oscillation. Needs a read-only
  `World::player_pos()` (player.rs `pos`) from the app between frames — reading state does not affect determinism
  because only the resulting word is recorded.
- **Virtual stick / d-pad** (option): fixed or floating centre, 8-way, dead zone; simpler, no state read.
- **Focus is never automatic** (changes speed, hitbox display, shot type): a held button, or a toggle (port setting).

### 3.3 Buttons, auto-fire, menus
- Buttons: SHOT, BOMB, FOCUS (hold/toggle), PAUSE (also Android Back, logical `NamedKey::BrowserBack`), SKIP
  (dialogue), plus optional Q/R shortcuts. Sized in mm via `scale_factor`, placed in the letterbox bars (§4).
- Auto-fire: SHOT held while scene = game and not paused/dialogue; menus are edge-driven, so auto-fire must be off
  there or a held SHOT would confirm on entry.
- Menu navigation: swipe = one direction pulse, tap = SHOT (confirm), two-finger tap / Back = BOMB/PAUSE (cancel).
  A pulse is ≥ 1 poll down then ≥ 1 poll up so the original edge detection sees it (each poll, not each frame —
  restarted calc chains poll again, host.rs comment).
- The launcher (egui) already handles touch through egui-winit; the in-game original menus are driven by the
  pulses above.

### 3.4 Bluetooth gamepads
- Android: buttons via `PhysicalKey::Unidentified(NativeKeyCode::Android(code))` + D-pad via arrow `KeyCode`s →
  a port mapping table (A=SHOT, B=BOMB, R1/L1=FOCUS, Start=PAUSE, Y=SKIP; editable). Analog stick: winit 0.30 turns
  joystick MotionEvents into `Touch` events — filter by `device_id`/source in a small android-activity input hook and
  read `AXIS_X/Y`, `AXIS_HAT_X/Y` (ndk `MotionEvent::axis_value`) → digital bits with a dead zone. gilrs stays for
  desktop only.
- Web: the Gamepad API through gilrs' wasm backend (already in the build).

### 3.5 Where the overlay is drawn
Port-only UI, never in core draw lists, and **never into the 640×480 frame texture**: `capture_screen`
(gfx.rs:610, 12 §1) copies that texture after present, so overlay pixels there would leak into the game's own
screenshots/fades and break the port-vs-port draw check. The overlay is a separate pass after `blit()` onto the
swapchain in window coordinates. Easiest implementation: egui (egui-wgpu already a dependency for the launcher) with
a painter-only layer: circles, labels, opacity setting; the overlay reads its layout from settings and the touch
state from `InputMux`.

## 4. Display

- **Landscape primary.** 20:9 phone 2400×1080: game 1440×1080 (×2.25), two 480 px bars → the stick zone and the
  buttons go in the bars, never over the playfield. **Portrait optional:** 1080×2400 → game 1080×810 (×1.69),
  1590 px below for controls; smaller game but one-handed. Orientation from a port setting (manifest
  `screenOrientation="sensorLandscape"` by default; web `screen.orientation.lock` in fullscreen).
- Scaling: today's linear letterboxed blit (gfx.rs `blit`) works; non-integer scale is fine at phone DPI. Optional
  integer + nearest mode is a renderer setting, not logic.
- Safe areas: letterbox inside `content_rect()` (cutouts, gesture bar); overlay buttons also inside it. Immersive
  fullscreen (hide system bars) via theme `@android:style/Theme.NoTitleBar.Fullscreen` + `layoutInDisplayCutoutMode`.
- **60 logic frames/s on 90/120 Hz screens.** Logic pacing stays Frame_Run's deadline/drop-late rule (00 §2, app.rs);
  presentation must not drive it. On Android call `ANativeWindow_setFrameRate(60.0, FIXED_SOURCE)` (ndk
  `NativeWindow::set_frame_rate`, API 30+) on every new window: 120 Hz panels switch to 60 or present each frame
  twice; 90 Hz panels that cannot switch show 3:2 judder (unavoidable; no interpolation, it would need draw state
  between logic frames). Present mode FIFO (AutoVsync).
- **Battery:** replace the 1 ms spin (§0) by `ControlFlow::WaitUntil(next_deadline)` on mobile (both Android and
  web, where requestAnimationFrame already paces), render only when a logic frame ran, and stop everything while
  suspended. Port setting "30 fps draw" (logic still 60) is possible because draw = emit after decision 2.

## 5. Lifecycle

| event | today | needed |
|---|---|---|
| `Suspended` (home, screen off) | not handled; `resumed()` returns early after the first time (app.rs:87) | drop the wgpu `Surface` (`Gfx.surface` is already `Option`), pause the cpal stream, stop calling `calc` (no polls → no logic frames: logic is simply frozen, like a dragged window on Windows) |
| `Resumed` again | ignored → renders to a dead surface | create a new surface for the new native window on the **same** device; re-apply `setFrameRate`; resume audio; reset the pacer deadline to now |
| auto-pause | — | on resume in scene = game and not paused, the mux injects one PAUSE pulse (as if Esc was pressed): the original pause menu opens; an input, so deterministic and legal in replays |
| GPU textures | live in the device | Vulkan/EGL devices survive surface loss → nothing to recreate normally. Device loss (rare) → rebuild textures from the core-owned `TexId` registry (step 8 `ResCmd`); render-target captures come back black until re-captured — acceptable |
| saves on background | score.dat only on exit (`exit_save`) | Android may kill a background process without `CloseRequested`. **`save_score` takes `&mut rng`** → saving from the live state would shift the RNG and desync the session. Save from a clone: `world.clone().exit_save_into(&mut host)` and discard the clone (decision 1 makes `World: Clone`). th08.cfg / port settings: write on change |
| process death mid-run | — | v1: lost run (like a crash). Later: serialize the cloned `World` on suspend (needs serde on core types; after step 11) |
| audio focus / route change | — | AAudio stream disconnect (headphones, call) → cpal error callback → reopen the stream on the next frame; Bgm/track position live in the mixer, so music resumes. Volume keys stay unhandled (winit default) |
| `LowMemory` | — | drop cached decoded music of non-current tracks (8 B/frame, §0) |
| web: `visibilitychange` hidden | rAF stops by itself | pause audio context, same auto-pause pulse on return |

## 6. What the rewrite must decide now, and effort

### 6.1 Cheap now, expensive later (fold into steps 7, 8, 8b, 10)
1. **Step 7 (now):** `RawInput` stays a merge of device words; the app owns an `InputMux` (keyboard + gamepad keys
   + touch → `keys[p]`). No platform types (winit `KeyCode`) in `Host`; no touch types in core. Saves/settings
   directories are given by the entry point (`PlatformDirs`), never `directories` inside settings.rs / midi.rs.
2. **Step 7:** a pure `Pacer` (Frame_Run deadline + drop-late, 00 §2) separate from the winit handler, with
   `suspend()/resume()` and `next_deadline()` so Android, web rAF and desktop share it; no `thread::sleep` in it.
3. **Saves are pure w.r.t. logic:** any save outside the original's call sites is taken from a clone (score.dat
   consumes RNG). Write this rule into the step-9 brief together with the clone round-trip test.
4. **Step 8:** the renderer separates device from surface (`attach(window)` / `detach()`), keeps a `TexId → source`
   registry (anm entry or bytes) for rebuild, and has a port-UI layer drawn after the blit, outside the 640×480
   frame texture.
5. **Step 8b:** content comes from an app-side `DataSource` trait (dir / APK assets / web files), not a cfg'd enum.
6. **Step 10:** app crate layout: `th08-app` (lib, `crate-type = ["cdylib", "rlib"]`) owns winit, wgpu, cpal, egui and
   exposes `run(platform)`; entry points: desktop `main.rs` (bin `th08`), `#[cfg(target_os = "android")] fn
   android_main(app: AndroidApp)` and the wasm `start(files)` in the same lib. `rfd`, `clap`, `env_logger`,
   `pollster`-blocking startup are desktop-only deps (`cfg(not(any(wasm32, android)))`); Android uses
   `android_logger`. Core keeps compiling for aarch64-linux-android (already true) and wasm32.
7. **W check:** add the Android `cargo check` (§1.1) now; it costs one deliverable (winit feature under
   `[target.'cfg(target_os = "android")'.dependencies]`, cfg the 5 `rfd` uses).

### 6.2 Can wait until after step 10
Touch overlay and its settings, gamepad axis hook, APK script, AAsset source, lifecycle handling, `setFrameRate`,
PWA manifest, orientation/fullscreen, World serialization for process death, music streaming instead of whole-track
decode, Android font path (`/system/fonts/NotoSansCJK-Regular.ttc` in text.rs list), integer scaling.

### 6.3 Effort (implementer runs, ≤ 3 deliverables each; after step 10)
| # | run | target |
|---|---|---|
| 1 | `android_main`, desktop-only deps cfg'd, `PlatformDirs` from `internal_data_path`, android_logger, font path | Android |
| 2 | `tools/android/build_apk.sh` (aapt2/zipalign/apksigner, `--data`), `ApkAssetSource` (AAsset fd), external-dir fallback | Android |
| 3 | lifecycle: surface detach/attach, audio pause/reopen, auto-pause pulse, save-from-clone on suspend | Android + web |
| 4–5 | `InputMux`, touch overlay (relative drag + stick, buttons, auto-fire, menu pulses), overlay settings in the launcher table | shared |
| 6 | gamepads: Android keycode table + axis hook | Android |
| 7 | pacing/battery: `WaitUntil`, `setFrameRate(60)`, optional 30 fps draw | Android + web |
| 8 | PWA manifest + service worker, fullscreen + orientation lock, audio unlock on first touch (`start_once` today is keyboard-only, app.rs:213), `storage.persist()` | web |

≈ 8 runs total (Android ≈ 6 with the shared runs, web ≈ 2 on top). Day-0 items of §6.1 add ≈ 1 deliverable now
(Android W check) and only API-shape rules to steps 7–10.

### 6.4 Testing
- No device on screen for agents: emulator headless (`emulator -avd … -no-window`, x86_64 image present → also add
  `x86_64-linux-android` target for it), `adb screencap`/logcat; the user tests on a phone (`adb install`, `scrcpy`).
- Determinism on ARM: the C0 pinned `fmath` is pure Rust (libm + table), so aarch64 must match x86 traces. Add a
  check after step 10: run the session tool for aarch64 under the emulator/`adb shell` (or qemu-user) and compare
  traces with native — the same idea as the planned wasm-in-node test.
- Touch-played replay → plays identically on desktop (the replay only has words).

## 7. Risks
- AAsset fd for a 450 MB stored entry: supported (stored entries only), but must be verified on a real device;
  fallback is extraction (§2.1).
- 90 Hz-only panels judder; acceptable.
- wgpu GLES fallback on old GPUs needs `downlevel_webgl2_defaults` (already what gfx.rs requests).
- Relative drag feels different from keyboard (digital 8-way under the hood); tune dead zone + sensitivity.
- Web on mobile: 497 MB in OPFS + wasm memory (47 MB archive + decoded track) may hit tab-kill limits on low-RAM
  phones; streaming music (§6.2) reduces it.
