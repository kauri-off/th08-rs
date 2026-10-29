# RE-Q62 — draw-log differences part 3 (spell-replay player sprite / time threshold, Extra clock sprite, etama single quads)

Code: th08.exe decompile/disassembly (addresses per row). Draw logs of the 20 batch500 drawlog recordings
(th08_ext_replays/ref500) and sessions/golden, used only to find the differences. Scratch port `re/work/runs/q62/portcopy`
= q61 portcopy (all RE-Q61 fixes) + the fixes below (CARGO_TARGET_DIR=target/re62; src/ untouched; env Q62NOQ1A /
Q62NOQ1B / Q62NOQ2 / Q62NOQ3A / Q62NOQ3B / Q62NOQ3C / Q62NOTOOL turn a fix off). Runs: re/work/runs/q62/{base,fix3,fix4}/ (fix2 abandoned: tool bug) (dc.sh, sum.py,
qd.py, lz.py, ch.py), CHECKPOINT.md. Before = tag base (q61 fixes + Q61 tool).

| # | recordings | original | spec | port |
|---|---|---|---|---|
| Q1a | spell replays, gameplay frame 1: ascii digits of the time row ("port-only digits", uv + colour) | GameMgr stage start (decomp line 28195 of all_decomp.c, in `GameManager_StageStart` 0x43ABD7): stats+0x40 := GameFlags bit 14 ? 0 : TIME_THR[stage][difficulty] — the only write | 11 §3.1 (already correct) | CODE BUG Q1a |
| Q1b | spell replays frame 1: player00.anm sprite additive 10 × 108 (original) vs alpha 70 × 108 (port) | `Player_RespawnUpdate` 0x44D180: f = f32(1 − GetF/30), scale x := 1 − f, scale y := 2f + 1, colour := (Timer_Get·255/30) << 24 \| 0xFFFFFF, blend bits := 1 (`0x44E0F0`) every call; end: scale 1, white, blend bits := 0 (`0x44E120`). `Player_DyingUpdate` 0x44CF5C: q = f32(GetF/30), scale y := 3q + 1, x := 1 − q, colour := ftol(255 − GetF·255/30) << 24 \| 0xFFFFFF, blend bits := 1. Spell practice starts in state 1 with timer 10 → visible at frame 1 | 09 §6.2, §6.3 | CODE BUG Q1b |
| Q2 | Extra (udLi14, 53406, 53020, udFa11) gm 450: times.anm#0 sprite [0,192,128,224] (original) vs [0,0,128,32] | `Hud_Load` 0x4390EE: clock VM impl+0x34D4 sprite := `GameMgr_GetClock` (stats+0x28) — 6 on an Extra fresh start (11 §3.2), or the replay record's clock (11 §3.4, loaded before the HUD registers, 11 §3.1) | 12 §5.1a | CODE BUG Q2 |
| Q3a | etama.anm#3 single quad at gm 12–13 (50948, 54840, udVa11) | `Player_SpawnInvulnRing` 0x40BF00: after `Effect_SpawnFixed` (whose `AnmFile_SetScript` already ran time 0, +0.003142) VM angular_vel.z *= −1.0 (0x4B438C) → rotation +v, 0, −v, … (present 226: original unrotated R, port rotated +2v) | 13 §2.6 row 23/24 | CODE BUG Q3a |
| Q3b | etama.anm#2 64-quad (79090 gm 3567, udFn15 gm 6096): spell start ring first frame, every vertex off by 8·(cos θ, sin θ) | `Spell_Start` 0x4152A0 writes only VM pos.y (+0x20C) := 64; VM pos.x stays 0, so the first update copies radius 0 | 07 §11.1 (clarified) | CODE BUG Q3b |
| Q3d | etama.anm#3 invulnerability ring scale one step behind (50948 present 227, 54840 217, udVa11 217) | `Player_SpawnInvulnRing` 0x40BF00 starts VM interpolation slot 4: elapsed +0x80 := 0, duration +0xD4 := copy of the player timer +0xE2AF4, mode +0xFC := 0 (linear), start +0x268 := scale, end +0x270 := (1/16, 1/16); `AnmVm_Run` advances it (05 §6) | 13 §2.6 row 23/24 (made explicit) | CODE BUG Q3d |
| Q3c | golden present 3677: one etama.anm#1 type-51/63 particle drawn by the port only | `AnmManager_EmitQuad` 0x4628B0 cull: draw iff vp.X ≤ max x, vp.Y ≤ max y, min x ≤ vp.X + W, min y ≤ vp.Y + H (f32, touching counts). The particle's min y ≈ 464.00 = viewport bottom: f32 rounding of the projection decides; covers no pixel either way | 05 §9 step 3 | TOOL: compare drops quads that cover no viewport pixel centre (scratch drawlog.rs `off_vp`) |
| — | golden 3677 (gauge aura at rotation π: original Q, port R with the same printed vertices) | — | — | not a difference (the parser turns both into the same quad) |
| — | uddo18 present 386: stg7bg.anm#0 3D quad, one vertex behind the near plane (z −201, x −1.42e6) off by 2e-4 relative | near-singular D3DXVec3Project (13 §2.6c: a port may use f32 division) | — | TOOL: vertices with z ∉ [0, 1] compared with 1e-3 relative tolerance (scratch drawlog.rs `compare`) |

## Effect (scratch draw-log compare; trace identical in every run)
Before = tag base (q61 fixes + Q61 tool), after = fix3 (Q1a, Q1b, Q2, Q3a, Q3b, both tool rules) and fix4 (+ Q3d).

| recording | before (base) | after (fix4) |
|---|---|---|
| ne_52233 | 3826(2:3693) etama.anm#1: changed 1 (pos 1) | 3826(2:3693) etama.anm#1: changed 1 (pos 1) |
| ne_54723 | 215(2:82) ascii.anm#0: changed 7 (pos 7) | 215(2:82) ascii.anm#0: changed 7 (pos 7) |
| ne_50948 | 226(2:94) etama.anm#3: changed 1 (pos 1) | 2371(2:2239) etama.anm#1: missing 1 extra 0 changed 0 |
| ne_79090 | 3699(2:3567) etama.anm#2: changed 64 (pos 64) | 3702(2:3570) ascii.anm#0: changed 2 (uv 2) |
| ne_54840 | 216(2:83) etama.anm#3: changed 1 (pos 1) | 286(2:153) stg1bg.anm#0: changed 33 (colour 33) |
| 8_udSe43 | 2362(2:2230) etama.anm#1: missing 0 extra 1 changed 0 | 4213(2:4081) ascii.anm#0: changed 2 (pos 2, uv 1) |
| ne_51890 | 216(2:84) ascii.anm#0: missing 0 extra 1 changed 0 | 216(2:84) ascii.anm#0: missing 0 extra 1 changed 0 |
| 8_udLi14 | 582(2:450) times.anm#0: changed 1 (uv 1) | 984(2:852) player01.anm#0: changed 1 (colour 1, 2D z/ |
| ne_53406 | 583(2:450) times.anm#0: changed 1 (uv 1) | 4659(2:4526) stg8bg.anm#0: changed 42 (colour 42) |
| ne_53020 | 582(2:450) times.anm#0: changed 1 (uv 1) | 5854(2:5722) ascii.anm#0: changed 2 (uv 2) |
| 8_udVa11 | 216(2:83) etama.anm#3: changed 1 (pos 1) | 420(2:287) stg5bg.anm#0: missing 2 extra 2 changed 0 |
| 8_uddo18 | 386(2:253) stg7bg.anm#0: missing 1 extra 1 changed 0 | 426(2:293) stg7bg.anm#0: missing 1 extra 1 changed 0 |
| 8_udFn15 | 6228(2:6096) etama.anm#2: changed 64 (pos 64) | 4110(2:3978) etama.anm#1: missing 1 extra 0 changed 0 |
| _udSi123 | 215(2:82) ascii.anm#0: changed 3 (uv 1, colour 3) | 306(2:173) ascii.anm#2: missing 1 extra 0 changed 0 |
| 8_udg342 | 208(2:82) ascii.anm#0: changed 3 (uv 1, colour 3) | 213(2:87) ascii.anm#0: missing 3 extra 0 changed 0 |
| 8_udKe02 | 208(2:82) player00.anm#0: changed 1 (pos 1, state 1) | 214(2:88) etama.anm#1: changed 1 (pos 1) |
| 8_udI811 | 214(2:82) ascii.anm#0: changed 3 (uv 1, colour 3) | 305(2:173) ascii.anm#2: missing 1 extra 0 changed 0 |
| 8_udGu02 | 215(2:82) ascii.anm#0: changed 3 (uv 1, colour 3) | 306(2:173) ascii.anm#2: missing 1 extra 0 changed 0 |
| 8_udFa11 | 582(2:450) times.anm#0: changed 1 (uv 1) | 6499(2:6367) ascii.anm#0: changed 2 (uv 2) |
| 8_udSr22 | 215(2:82) player00.anm#0: changed 1 (pos 1, state 1) | 306(2:173) ascii.anm#2: missing 1 extra 0 changed 0 |
| golden | 3677 (2:2991) etama.anm#1 extra 1 | 4439 (2:3753) etama.anm#1 changed 1 (pos) — 13 §2.6c open question |

Trace identical in all 20 + golden (8000 frames); identical-frame count up in every recording. Q3d alone (fix3 → fix4): 50948 227 → 2371, 54840 217 → 286, udVa11 217 → 420.

## CODE BUG list (src function → change → spec)
- Q1a `src/enemy.rs` `EnemyManager::stage_start`: time_threshold := 0 when GameFlags bit 14 (spell practice), else
  TIME_THR[stage][difficulty] (11 §3.1).
- Q1b `src/player.rs` `Player::dying_update`: every call before the t > 29 test: q = f32(GetF/30) (timer cur + frac),
  scale = (1 − q, 3q + 1), colour := ftol(255 − GetF·255/30) << 24 | 0xFFFFFF (RGB white too), VM blend bits 4–5 := 1
  (09 §6.2). `Player::respawn_update`: f = f32(1 − GetF/30), scale = (1 − f, 2f + 1) (x and y differ), colour :=
  (Timer_Get·255/30) << 24 | 0xFFFFFF, blend := 1 every call; at the end blend bits := 0 (09 §6.3).
- Q2 `src/hud.rs` `Hud::load`: clock sprite from the live stats (player stats, after `GameManager::stage_start`'s
  mem::take), not the emptied `ctx.game.stats` copy (12 §5.1a).
- Q3a `src/effect.rs` `invuln_ring`: after the spawn VM angular_vel.z *= −1 (13 §2.6 row 23).
- Q3b `src/effect.rs` `spell_ring`: write only vm.pos.y := 64, keep pos.x (07 §11.1).
- Q3d `src/effect.rs` `invuln_ring` + effect update `23 | 24`: start VM scale slot 4 (duration = frames, linear, from
  current scale to 1/16) at the spawn; drop the hand-computed scale in the type 23/24 update (13 §2.6 row 23, 05 §6).
- TOOL `src/drawlog.rs` `items`/`off_vp`: drop quads entirely ≥ 0.5 px outside the viewport (cover no pixel, 05 §9 step 3),
  after the strip split, on both sides; `compare`: vertices with z ∉ [0, 1] get 1e-3 relative position/z tolerance.

## Open questions
- 13 §2.6c projection precision: the 0x428310 push test (|d2|² < 7744, strict) flips on golden present 4439 (and very
  likely 52233 present 3826, same particle class) because the port projects with f32 division and the original with
  D3DX8 SSE (`rcpps` + one Newton step, M = (world·view)·proj). Emulating it bit-exactly needs the reference CPU's
  `rcpps` table (AMD; vendor-specific, not specified). Decide: emulate D3DX8 SSE arithmetic with a documented rcpps
  model, or accept the one-frame 1.75 px difference (draw only, logic unaffected).
- 50948 present 2371 etama.anm#1 missing 1, 54840 286 stg1bg colour, udVa11 420 stg5bg z/zwrite, and the other firsts
  above: not analysed (next RE question).
