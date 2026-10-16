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
- Dirty room = Opus 5.5 (claudeAgent, effort medium) + GLM-5.3-Flash (opencode, max), run as T3 delegate_task, same brief. DeepSeek dropped 2026-10-17 (cost); Haiku 5.5 test failed.
- RE tool = Ghidra headless (Hopper proved unreliable). Helpers in specs/ghidra_tools.
- REA MCP registered for claude_code (~/.claude.json, backup .rea.backup) and opencode, both with JAVA_HOME=/usr/lib/jvm/java-21-openjdk
  + GHIDRA_INSTALL_DIR=/opt/ghidra. Before that REA's ghidra provider was unavailable (system default Java 17) — agents only had Hopper.
