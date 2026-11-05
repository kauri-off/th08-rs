# Vendored crates

`android-activity` 0.6.1 (MIT OR Apache-2.0, https://github.com/rust-mobile/android-activity), patched in through
`[patch.crates-io]`. winit turns a joystick motion event into a touch move with AXIS_X / AXIS_Y only and marks it
handled, so a d-pad sent as AXIS_HAT_X / AXIS_HAT_Y never reaches the app. The patch adds
`input::set_motion_hook`, called with every motion event before winit's callback (`src/input.rs`,
`src/native_activity/mod.rs`). `src/game_activity` and the bundled games SDK are left out: th08 builds the
NativeActivity backend only.

Second patch: winit ignores `MainEvent::Destroy`, while `onDestroy` blocks the Java UI thread until `android_main`
returns, so a destroyed GameActivity hung the game (ANR). The patch adds `destroy_requested()`, set when the Destroy
command reaches the main loop (`src/lib.rs`, `src/native_activity/glue.rs`); the app polls it in `about_to_wait`,
saves and leaves the event loop, and `android_main` ends the `:game` process.
