# Decisions

- Rust 2024, winit 0.30 + wgpu 30 (WebGPU/WebGL2/Vulkan) + cpal (audio, webaudio on wasm). anyhow for errors.
  Why: user wants a WebAssembly build. macroquad was dropped (2026-10-16).
- Logical screen 640x480, letterboxed via viewport; sprite batcher with Alpha/Add pipelines (src/gfx.rs).
  Surface format forced to 8-bit (Bgra8Unorm/Rgba8Unorm) — the default picked Rgb10a2 and broke capture.
- Assets are ZUN's: never hosted. Native: game dir path. Web: user selects folder/files; cached in OPFS;
  thbgm.dat read per track via File.slice (no full 450 MB load). Without thbgm.dat the game runs silent.
- All asset IO is async through assets::Source (native = blocking fs, web = Blob slices).
- Assets are loaded at runtime from the user's original th08.dat / thbgm.dat — nothing is copied into the repo.
- Fixed-timestep 60 Hz logic; rendering once per display frame; at most 4 catch-up steps.
- Dirty room = Opus 5.5 only (claudeAgent, effort medium), run as T3 delegate_task. GLM/DeepSeek/Haiku dropped 2026-10-17; their dirs deleted.
- RE tool = Ghidra headless (Hopper proved unreliable). Workspace re/ (helpers re/tools, scripts re/ghidra_scripts).
- REA MCP registered for claude_code (~/.claude.json, backup .rea.backup) and opencode, both with JAVA_HOME=/usr/lib/jvm/java-21-openjdk
  + GHIDRA_INSTALL_DIR=/opt/ghidra. Before that REA's ghidra provider was unavailable (system default Java 17) — agents only had Hopper.
- Joystick (impl68) = gilrs (browser Gamepad API on wasm). On Linux gilrs needs libudev-dev at build time, so it is
  behind the `gamepad-linux` feature (`cargo build --release --features gamepad-linux`); the default Linux build and the
  AppImage CI have no joystick until libudev-dev is added there.
