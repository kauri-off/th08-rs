# Decisions

Technical choices and why they were made.

- Rust 2024, winit 0.30 + wgpu 30 (WebGPU/WebGL2/Vulkan) + cpal (audio, WebAudio on wasm), anyhow for errors.
  Why: a WebAssembly build was a goal from the start. macroquad was tried first and dropped.
- Logical screen 640x480, letterboxed via the viewport; sprite batcher with Alpha/Add pipelines
  (`crates/th08-render/src/gfx.rs`). The surface format is forced to 8-bit (Bgra8Unorm/Rgba8Unorm): the default pick
  was Rgb10a2 and broke screen capture.
- Game assets are ZUN's and are never hosted or copied into the repository. They are read at runtime from the user's
  own `th08.dat` / `thbgm.dat`. Native: a game folder path. Web: the user selects the folder or files, which are cached
  in OPFS; `thbgm.dat` is read per track via `File.slice` (no full 450 MB load). Without `thbgm.dat` the game runs
  silent.
- All asset IO is async through one source interface (native = blocking fs, web = Blob slices).
- Fixed-timestep 60 Hz logic; rendering once per display frame; at most 4 catch-up steps.
- Reverse-engineering tool: Ghidra headless (Hopper proved unreliable on this binary). Workspace `re/` (helpers in
  `re/tools`, Ghidra scripts in `re/ghidra_scripts`).
- Joystick: gilrs (the browser Gamepad API on wasm). On Linux gilrs needs libudev-dev at build time, so it sits behind
  the `gamepad-linux` feature (`cargo build --release --features gamepad-linux`); the default Linux build has no
  joystick support.
- Behaviour parity rules: [PARITY_POLICY.md](PARITY_POLICY.md). Architecture of the idiomatic rewrite:
  [arch/ARCH_SUMMARY.md](arch/ARCH_SUMMARY.md).
