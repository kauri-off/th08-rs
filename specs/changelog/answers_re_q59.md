# RE-Q59 — the 2 batch2 logic divergences (udBu09, udOa25) + the main-menu fade quad colour

Code: disassembly of th08.exe (addresses per row). Traces / draw logs (ref_b2, ref500 recordings; original re-runs
re/work/runs/q59/orig_* with TH08_RNGSITE / TH08_SCAN + SCANEFF / SCANEFFSLOT; port dumps Q59EFF / Q59SLOT /
TH08_SESSION_RNG) only to locate. Scratch port `portcopy` = master b93b569 + the fixes below + debug dumps
(CARGO_TARGET_DIR=target/re59; src/ untouched; env Q59NOFIX / Q59NOAU / Q59NOAV switch a fix off). Runs and notes:
re/work/runs/q59/CHECKPOINT.md, fixATAU.tsv, fixAll.tsv, dc_*.txt.

| # | replay / row | original | spec | port |
|---|---|---|---|---|
| AT | gensokyou_th8_udBu09 (1.00b spell practice card 221, 4A) r8492: port rng_ctr −8 from frame 8303 on | Frame 8303: `Ecl_Run` op 140 (0x41D17E) → `Effect_SpawnWithParam` type 26 (6 script + 2 `EffInit_Burst` draws); the port's 512-slot ring is full, so no spawn. Ring diff: slot 284, a graze spark (type 8, script 40, spawned in `BulletMgr_Calc` frame 8191 at game speed 0.25; speed 1 at 8192) is freed at 8302 in the original, 8299 in the port. The original starts the script inside the spawn (`Effect_Spawn` +0x10B → `AnmFile_StartScript` → `AnmVm_Run`, clock += speed at the spawn = 0.25); the port's `spawn_rng` defers the script start to the next `EffectMgr_Calc`, where it runs with that frame's speed (1.0): clock 0.75 ahead. Also `EffInit_Spark` 0x425D70 scales v, a by g_GameSpeed at the spawn; the port passes 1.0 | 13 §2.2 "Spawn-time game speed" (new) | CODE BUG AT |
| AV | gensokyou_th8_udOa25 (full SkRr Lunatic 6B) r30456 (stage 2 frame 14071 rng ±4, rows 30501–30505 time/score ±1, then equal again) | Frame 14071: the original frees ring slot 176 (type 51, age 127) in `EffectMgr_Calc`, the port keeps it one frame → the frame's 2nd graze spark finds no slot in the port (spark one frame later, items one frame earlier). Particle state (world, v, a, camera, stored facing) identical through 14070 (orig_Oa25s P lines vs port Q59SLOT). `Vec3_Dot` 0x40B540 disasm: `fld [ecx]; fmul [edx]; fld [ecx+4]; fmul [edx+4]; faddp; fld [ecx+8]; fmul [edx+8]; faddp` = (x·x' + y·y') + z·z'. Values in 13 §2.6a: (x+y)+z = 0x3F70A3D6 < 0.94 (freed); the port's (z+y)+x = 0x3F70A3D7 = f32 0.94 (kept). The compare is `fcomp dword [0x4B4988]` (f32 0.94) after `fstp` f32 | 13 §2.6a (order corrected; RE-Q52 had (z·z' + y·y') + x·x') | CODE BUG AV |
| AU | all 20 drawcheck recordings: main menu present 57–62 call 15, full-screen quad fbffffff (ref) / ffffffff (port), ~70 frames | Title fade = ScreenEffect type 0, p0 70, colour 0xFFFFFF, registered by `MainMenu_LoaderThread`. `ScreenEffect_Register` 0x45B8B0 zero-fills the 0x34-byte object (alpha +0x10 = 0); type 0 calc 0x45B160: alpha = ftol(255 − t·255/p0) (≥ 0), then t ≥ p0 → removed, else tick; draw 0x45BB50 colour = +0x10 << 24 \| p1. The colour formula is the port's too; the reference sequence is ff, fb, f7, f4 … 03 from the fade's first frame, the port's ff, ff, fb … (one frame late): the port queues it with `effect::screen` (linked after the calc pass → drawn once before its first calc) and `screen_fx` presets alpha 255 (type 0) / A (type 3) for that pre-calc draw, where the original object would draw alpha 0 | 13 §3 "Initial alpha", "Title fade" (new); 12 §1 hold (2) | CODE BUG AU |

Q1/Q2 share the mechanism (the 512-slot effect ring is full and an effect is freed one frame earlier/later in the port,
so a later spawn of that frame lands or not, with its RNG draws) but not the cause: AT is the spark's VM clock under a
game-speed change, AV the dot-product order of the type 51 cull.

## Effect (scratch, same binary)
- AT: udBu09 identical (8934 rows). AV: udOa25 identical (148747 rows).
- AT + AU: sample of 50 identical recordings (re/work/runs/q59/sample50.txt: 25 batch2 + 25 batch500, seed 59) 50/50
  identical (fixATAU.tsv).
- AT + AU + AV: 74/74 identical (fixAll.tsv: sample50 + the 22 pilot recordings incl. silentselene_80406, the RE-Q52
  case, + udBu09 + udOa25).
- AU draw log (session draw-log compare vs master target/reb2): udg342, udI811, udKe02, udSi123: colour changes −69
  each, identical frames +62…+67, no other count changes; the first draw divergence moves from present 57–60 to
  120–127 (udg342 / udKe02: an extra title01.anm#2 quad in the port at present 120 — not analysed).
