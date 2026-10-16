# Decisions

- Rust 2024, macroquad 0.4 (window, GL textures, simple 2D) + cpal (audio). anyhow for errors.
  Why: minimal glue, pure Rust; macroquad materials allow custom blend modes needed for additive sprites.
- Assets are loaded at runtime from the user's original th08.dat / thbgm.dat — nothing is copied into the repo.
- Fixed-timestep 60 Hz logic; rendering once per display frame; at most 4 catch-up steps.
- Dirty room = opencode agents (GLM-5.3-Flash, DeepSeek V4.1 Flash), reasoning max, run as T3 delegate_task, same brief in parallel.
- RE tool = Ghidra headless (Hopper proved unreliable). Helpers in specs/ghidra_tools.
