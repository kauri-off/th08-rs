# IMPL60 questions

1. **GM draw after a stage clear (11 §3, §4 step 1).** The clear frame sets loading (GM+0x38) := 1 and no loader
   thread runs for that GM, so loading stays 1 on every frame until the scene switch (practice result, 6A/6B/Extra,
   night-over ending: one frame or until SHOT/ENTER). What does the GM draw node 0x43AA03 (prio 5) do on those
   frames: return 3 like during a real load (the stage, bullets and HUD are then not drawn, only the loading-screen
   prios 0–2), or draw normally? The port keeps drawing the stage (`gamemgr.rs` SPEC-GAP in `calc`).
2. **0x438F58 stage-transition display (11 §4 step 1).** "If `next_scene_after < 0`: HUD 0x438F58" has no
   description in 12. What does it start (VMs, text, duration), and does it run on the replay path too? Not
   implemented.
