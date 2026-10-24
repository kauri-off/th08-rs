# 11 — RNG, game manager flow, scenes, stats block

Sources: th08.exe via Ghidra (names in `symbols.csv`). VERIFIED = read from code; INFERRED = interpretation.
Global objects: `g_Rng` 0x164D520, `g_GameManager` (GM) 0x160F508, `g_Supervisor` 0x17CE758, stats block
= `*(GM+8)`, config copy = `*(GM+4)`.

## 1. RNG — VERIFIED

### 1.1 State and algorithm
| off | type | meaning |
|---|---|---|
| +0 | u16 | state `s` |
| +4 | u32 | call counter (incremented by every Next16; reset to 0 by the replay hook §1.4 on every frame the hook node is linked; never read by any code; **not** the traces' `rng_ctr` column, §1.4) |

```
next16():  t = (s XOR 0x9630) − 0x6553         (mod 2^16)
           s = ((t << 2) | (t >> 14)) & 0xFFFF  (rotate left by 2)
           counter += 1; return s
next32():  hi = next16(); lo = next16(); return hi << 16 | lo        (first call = high half)
```

Derived functions (only one RNG object exists; all 125 call sites pass `&g_Rng`):

| name | addr | result |
|---|---|---|
| `Rng_Next16` | 0x43ECC0 | as above |
| `Rng_Range16(n)` | 0x410C00 | n == 0 → 0 (no draw); else `next16() % n` (n is u16) |
| `Rng_Next32` | 0x43ED20 | as above |
| `Rng_Range(n)` | 0x406EF0 | n == 0 → 0 (no draw); else `next32() % n` (unsigned) |
| `Rng_Float` | 0x43ED50 | `next32() / 4294967296.0` ∈ [0, 1] (see 1.2) |
| `Rng_FloatRange(x)` | 0x40D390 | `Rng_Float() · x` |
| `Rng_Signed` | 0x43ED80 | `next32() / 2147483648.0 − 1.0` ∈ [−1, 1] |
| `Rng_SignedRange(x)` | 0x4143C0 | `Rng_Signed() · x` |
| `Rng_GetState` / `Rng_SetState` | 0x453C20 / 0x453BE0 | read / write `s` (counter untouched) |
| `Rng_ResetCounter` | 0x453C00 | counter = 0 |

### 1.2 Floating-point precision — VERIFIED (runtime, changelog/answers_phase8.md §1)
The u32 is loaded exactly (64-bit integer load; precision control does not affect loads) and divided on the x87.
The CRT start-up sets 53-bit precision (`_controlfp(0x10000, 0x30000)`, 0x4A72D7). The control word was measured
under wine:
- the main thread has 0x027F until `CreateDevice`;
- after `CreateDevice` (no FPU_PRESERVE) the main thread has **0x007F**: 24-bit precision, round to nearest;
- 0x007F holds for the whole Frame_Run (calc chain, draw chain, Present).

So `Rng_Float()` = `(f32)(u32) / 2^32`, rounded to nearest-even. Values ≥ 0xFFFFFF80 round to **1.0**.
`Rng_FloatRange(x)` = single-precision product. Equivalent Rust: `(n as f32) / 4294967296.0_f32 * x`.

**Loader threads keep 0x027F (53-bit).** `GameManager_StageStart` runs on the loader thread (`CreateThread` via
0x4486BA), which has its own FPU state. The measured CW there is 0x027F for every stage start, and RNG calls
are made from that thread.
- RNG draws there are integer (Range16, Range, Next32), or feed decoys (`Rng_Float` in the new-game decoys,
  §1.5). Neither is affected.
- Float math on that thread that keeps intermediates in x87 registers (multi-operation expressions, fsin/fcos,
  int→float of values > 2^24) is rounded to 53 bits, not 24.
- A single +,−,×,÷ whose result is stored straight to a float32 gives the same value under both precisions.
- Port rule: compute stage-start/loader-thread initialisation in f64 and store as f32 only where the original
  stores. Compute everything on the main thread in f32 per operation.

### 1.3 Seeding
- Boot (0x4464C8): `s = (u16) timeGetTime()`. **No reseed on new game** — a new game continues from whatever
  state the menus left.
- Stage start (`GameManager_StageStart` 0x43ABD7), after all GM/player/stats initialisation:
  `stage_seed (0x164D2C4) = s`. Recording replays store this value per stage (record +0x1A, §6).
  It is captured **before** the stage subsystems are registered (Stage, Bullets, Enemies (2 draws, §1.5),
  Effects, HUD, Gui), so the same post-seed sequence must be reproduced.
- Replay playback: replay stage load (0x452D60) sets lives/bombs/power from the record (SetLives and SetPower
  each consume RNG), then `Rng_SetState(record.seed)`; StageStart then performs one checksum update with the
  RNG saved and restored around it, and captures `stage_seed` — identical to the recorded state.
- Practice / spell practice use the same path (no special seed).

### 1.4 Per-frame hook — VERIFIED (RE-Q29)
`Replay_FrameHook` 0x4522A0 is a calc node at priority 7. `Replay_Register` (called by `GameManager_StageStart`
after `Gui_Register`, 11 §3) creates and links it only when it creates the replay object, i.e. at the first stage
of a game; later stages reuse the object and node. While **recording** (mode 0) it is always linked; during
**playback** (mode 1) only when the replay header byte +6 ≠ 0 (debug format, 15 §3.1; every recorder writes 0, so
never for retail or self-recorded files). On linking, `Replay_Register` also calls the hook once directly. The
node has no gate of its own (no GM-flag, pause or bit-9 test): it runs on every frame the calc chain reaches
priority 7. Each run, in this order:
1. replay object +0xDA (statistics word, 08 §9) := 0;
2. replay object +0xD8 := `s` (u16 RNG state);
3. RNG call counter (g_Rng+4) := 0;
4. if `GM+0x3DB98` (0x164D0A0) ≠ 0: +0xDA |= 0x100; then `GM+0x3DB98` := 0. `GM+0x3DB98` is set to 1 only by the
   pause entry (11 §4 step 4, 0x43A153).

**None of these values is ever read** (VERIFIED by byte search for 0x164D524 / 0x164D0A0 and every replay-object
access): g_Rng+4 is written only by `Rng_Next16` (+1) and `Rng_ResetCounter` 0x453C00 (:= 0, called only by the
hook); +0xD8 and +0xDA have no readers. The hook therefore has **no observable effect** on gameplay, files or
display; a port may implement it or omit it.

The `rng_ctr` column of every recorded trace (sessions, demo traces; specs/data/README.md,
specs/data/drawlog/README.md) is **not** g_Rng+4: it is the probe's own count of all `Rng_Next16` calls since
process start, which nothing resets. In those traces it therefore grows through gameplay even though the
hook zeroes g_Rng+4 every recorded frame. A port that resets its counter in the hook must keep a separate,
never-reset count for that trace column.

### 1.5 RNG consumers (all VERIFIED call sites; "draws" = Next16 calls)

| consumer | when | draws |
|---|---|---|
| `GameMgr_UpdateChecksum` 0x406E50 | every SetLives/AddLives, SetPower/AddPower, AddBombs, AddTime(n ≠ clamp case), death/bomb counters (0x44E260/0x44E2E0), point-item collection (0x440E40), spell capture (0x4161B0), stage start (several), pause (RNG restored afterwards) | 2 × Rng_Range(100000) = 4 |
| `GameMgr_ScrambleDecoys` 0x44E160 | each player hit test that kills (0x44A230 bullets, 0x44A360 enemies, 0x44A6A0 lasers) | 5 × Range + 3 × Float = 16 |
| new-game decoys 0x43B984 | new game / practice start (§3.2) | 21 × Next32 + 9 × Float = 60 |
| new-game heap jitter | new game, before 0x43B984: `malloc(Next32 % 0xFFFF + 16)` | 2 |
| `EnemyManager_Load` | each stage load: Range16(3), Range16(8) (enemy-manager fields +0x9DCDC0/+0x9DCDC2) | 2 |
| ECL | spawn ops 2/3/5, vars 10032–10035, 10082, math 8/9, ops 67, 106, 142, 168, 169, 178, aim modes 6–8 (07) | var. |
| bullets | `PlaceBullet` random modes (08) | var. |
| items | spawn modes 2/3/5 (08 §8), death drops | var. |
| player | shot init 7, update 3, hit 1 (09 §9.4); bomb functions of Reimu LS, Sakuya/Remilia bombs, Yuyuko LS | var. |
| ANM VM | ops 59/60 (05) in **any** running script (gameplay effects, HUD, menus) | var. |
| EffectManager | effect update functions 0x425D70…0x4270C0 | var. |
| ScreenEffect shake (types 1, 7) | 2 × Rng_Range(3) per frame | 4/frame |
| enemy helpers | 0x422020 (op 67/178), 0x4224A0, 0x42ADB0, `Enemy_DropItems` 0x42BEA0, timeline 0x42A8A0 | var. |
| replay save 0x4531F0 | after the game (key, 2 random bytes) | 3 × Range16(256) = 3 (Phase 6, 15 §3.5) |
| score.dat save 0x453D0D | every Result removal, incl. mode-2 flushes at game start / pause-quit / exit | 2 × Range16(256) = 2 (15 §2.1) |

Pause must not consume RNG: the only RNG use on pausing is the checksum update, wrapped in save/restore.

## 2. Game manager object (GM 0x160F508) — fields used by the flow

| off | abs | type | meaning | conf |
|---|---|---|---|---|
| +0x00 | 0x160F508 | ptr | dummy block (heap jitter, freed immediately) | VERIFIED |
| +0x04 | 0x160F50C | ptr | copy of the 60-byte config (0x17CE870); +0x1C = starting lives, +0x25 = slow-mode option | VERIFIED copy / INFERRED names |
| +0x08 | 0x160F510 | ptr | stats block (0xE4 bytes, §5) | VERIFIED |
| +0x2C | 0x160F534 | u8 | freeze flag (HUD calc, enemy timers, bullet movement, player calc, spell calc skip while ≠ 0). Only writer that sets it: ECL native 26 (= arg). Cleared by `Player_Die` and `GameManager_StageStart` (changelog/answers_impl20.md Notes) | VERIFIED |
| +0x2D | 0x160F535 | u8 | slow-mode skipped frame (input is OR-accumulated by the Supervisor while set) | VERIFIED |
| +0x30 | 0x160F538 | u32 | difficulty 0 Easy, 1 Normal, 2 Hard, 3 Lunatic, 4 Extra | VERIFIED (tables) / names INFERRED |
| +0x34 | 0x160F53C | u32 | difficulty mask (1 << d, Extra = 0xF) | VERIFIED |
| +0x38 | 0x160F540 | u32 | loading state: 0 playing, 1 loading, 2 load failed | VERIFIED |
| +0x3C | 0x160F544 | u32 | frames spent in the loading state | VERIFIED |
| +0x3DA9C | — | i32 | deaths (this game) | VERIFIED |
| +0x3DAA8 | — | i32 | pauses | VERIFIED |
| +0x3DAAC | — | i32 | total time added | VERIFIED |
| +0x3DBA8 | 0x164D0B0 | u8 | (reset at new game / death; replay record +0x20) | VERIFIED storage |
| +0x3DBA9 | 0x164D0B1 | u8 | shot type 0–11 (04) | VERIFIED |
| +0x3DBAA | 0x164D0B2 | u8 | added to shot type into +0x3DBAB (+0x3DBAB has no reader). 0 except during attract demos, where it is shot % 2 (changelog/answers_phase7.md §3) | VERIFIED |
| +0x3DBAC | 0x164D0B4 | u32 | **game flags** (§2.1) | VERIFIED |
| +0x3DBB0 | 0x164D0B8 | i16 | spell id for spell practice | VERIFIED |
| +0x3DBB2 | 0x164D0BA | u8 | paused (1 = pause menu open; 2 after it was drawn once) | VERIFIED |
| +0x3DBB3 | 0x164D0BB | u8 | game-over / continue menu open | VERIFIED |
| +0x3DBB4 | 0x164D0BC | u8 | demo replay index 0–3 | VERIFIED |
| +0x3DBB8 | — | i32 | demo frame counter | VERIFIED |
| +0x3DDC0 | — | u32 | gameplay frame counter (+1 per non-paused frame) | VERIFIED |
| +0x3DDC4 | 0x164D2CC | u32 | stage index 0–8 (04) | VERIFIED |
| +0x3DDC8 | 0x164D2D0 | u32 | copy of stage index at stage start | VERIFIED |
| +0x3DDD0 | 0x164D2D8 | u16 | 1 << stage (clear-mask bit) | VERIFIED |
| +0x3DDD4..+0x3DDF0 | 0x164D2DC.. | f32×8 | playfield rect (32, 16, 384, 448) and player clamp rect (8, 16, 376, 432) | VERIFIED (0x43C686) |
| +0x3DDF8.. | — | i16 | gauge thresholds (09) | VERIFIED |
| +0x3DE28 | — | i32 | deferred music start (1 = play stage BGM 0) | VERIFIED |
| +0x3DE2C/30/34/38 | — | i32 | rank, max, min, sub-counter (07/09); boot values **0, 0, 0, 0**: `GameMgr_Ctor` 0x43C5E1 (static init) zeroes GM +0..+0x3DE3B, then sets only the playfield rect +0x3DDD4..+0x3DDE0 = (32, 16, 384, 448) and demo index +0x3DBB4 = 3; the first `GameMgr_InitRank` (§3) writes the real values (RE-Q24) | VERIFIED |

Gameplay frame counter 0x164D2C8 (GM+0x3DDC0): += 1 near the end of `GameMgr_Calc` (not on its early returns: loading, pause/game-over, slow-mode skip); := 0 by `GameManager_Register` and `GameMgr_OnRemove` (its last statement, after the subsystem teardown and `Ascii_InitVms`; VERIFIED 0x43BE2C, RE-Q28), so it reads 0 in the menus after a pause-quit (changelog/answers_impl21.md §Q3). Globals next to it: 0x164D30C play-time counter (frames: += 1 at the top of every `EnemyMgr_Calc` call while no dialogue is active (`Msg_IsActive` == 0), before the bit-10 test, in the same block as the human-rate counters (15 §2.3) — so not on paused / game-over-menu / loading / post-clear frames, where `GameMgr_Calc` returns 3 before the enemy manager's calc, but yes during the bit-10 death freeze; + 1800 per ECL op 148; := 0 by StageStart; VERIFIED 0x42C6A0, RE-Q24); 0x164D318/0x164D31C per-frame
counters not advanced during dialogue.

### 2.1 Game flags (GM+0x3DBAC = 0x164D0B4) — bits seen in code

| bit | meaning | conf |
|---|---|---|
| 0 | stage practice | VERIFIED (lives 8, power per stage, ends with result) |
| 1 | demo play (attract mode) | VERIFIED (any key → menu; time limits) |
| 2 | gameplay running: **recomputed every `GameMgr_Calc`** at step 7 of §4 (after the pause-key test of step 4, among the anti-tamper checks, before the step-8 pause return; not on the frames that return earlier: loading, demo end, replay slow-mode skip) as paused byte GM+0x3DBB2 (0x164D0BA) == 0 and game-over byte GM+0x3DBB3 == 0, so it drops on the ESC frame and returns on the first calc after unpausing; also set at stage start, cleared by `GameMgr_OnRemove` (changelog/answers_impl21.md §Q3) | VERIFIED |
| 3 | replay playback | VERIFIED |
| 4 (0x10) | ending-eligible / "cleared without running out of night" (set on final clear, cleared on time-out endings) | VERIFIED writes / INFERRED meaning |
| 5–6 | stage-end state: 0 none, 1 waiting-for-load (after a clear), 2 clear triggered, 3 clear handled | VERIFIED |
| 7–8 | 7 = a boss "last spell" is running (ECL op 176 clears 7/8/13 and sets 7); `Spell_End`: captured → bits 7–8 := 8 only, failed → cleared (+ "Last Spell Failed" if spell flag bit 5); bit 8 then persists until the next op 176, the dissolve-spell end or `StageStart` (clears 7, 8, 13). While either is set a bomb press only plays SE 0x29 and a hit forces the dissolve spell (09 §7, changelog/answers_impl10.md §7, changelog/answers_impl11.md §14) | VERIFIED |
| 9 (0x200) | stage-clear bonus panel shown (msg op 9) | VERIFIED |
| 10 (0x400) | spell-card death freeze: set by `Player_Die` 0x44AB40 only when a spell card is active (0x4178A0 ≠ 0), together with the anm tint; cleared when the deathbomb window ends in a real death (`Player_DyingUpdate`, with `AnmMgr_TintOff`), when a (death)bomb starts with GameFlags bits 7–8 clear (`Player_UpdateBomb`), and at `StageStart`. While set: `Stage_Calc` returns at once (06 §5: no STD script, camera, sway, fog, objects, VMs, sparkles, frame counter), `BulletMgr_Calc` and the player bullet update are skipped (09 §4.4), `EnemyMgr_Calc` does nothing but its play-time / human-rate counters (07 §10.1 step 0, VERIFIED RE-Q28), and enemies on prio 11 draw tinted (07 §10.6). VERIFIED (RE-Q24) | VERIFIED |
| 11–12 | final route choice (0 → 6A, else → 6B) | VERIFIED |
| 13 (0x2000) | set by op 176 together with bit 7 when (not practice and stage idx 6/7) or (practice and card id 0x8F–0x92 or 0xAB–0xBE); with a boss present `EnemyMgr_Calc` adds a large player-damage box (changelog/answers_impl10.md §7) | VERIFIED |
| 14 (0x4000) | spell practice | VERIFIED |
| 15–17 | unlock states from the score file: 15 Extra (`Clear_ExtraUnlocked`), 16 spell practice (`Clear_AnyFinalA`), 17 solo characters (`Clear_AllTeamsFinalB`) (16 §4 table) | VERIFIED |

## 3. New game and stage start (`GameManager_StageStart`, loader thread) — VERIFIED

`GameManager_Register` (0x43AA5C) adds calc node prio 2 (`GameMgr_Calc` 0x439BC7, on-register 0x43AAF4,
on-remove 0x43BE2C) and draw node prio 5 (0x43AA03). The on-register callback sets `loading = 1`, decides
whether this is a **fresh start** (`0x17CE8BC = 1` unless the requested scene was 3/11/12), sets up the
loading screen, forces flags bits 5–6 to 1 if they were ≥ 2 (stage cleared → wait), and starts
`StageStart` on a **separate thread**.

### 3.1 Every stage start
```
loading_frames = 0
stage_mask = 1 << stage; stage_copy = stage
game_speed = 1.0; flags &= ~0x400
[3.2 new game | 3.3 continuing]
playfield/clamp rects (0x43C686)
0x164D340 = 0; stats.point_items_stage = 0 (+0x2C); stats.graze_stage = 0 (+0x04)
    # 0x164D340 = GM+0x3DE38 = the rank sub-counter (§2): reset at **every** stage start, after both the 3.2 and the
    # 3.3 branch, while rank / min / max are set only by GameMgr_InitRank (fresh start, replay playback) — a
    # continuing stage keeps the rank but restarts the sub-counter at 0 (VERIFIED 0x43B1B1, RE-Q29)
paused = 0; flags &= ~0x2180; counters 0x164D31C/328/32C = 0
stats.gauge_start (+0x20) = stats.gauge (+0x22)
stats.time (+0x3C) = 0; stats.time_parity (+0x44) = 0
stats.time_threshold (+0x40) = spell practice ? 0 : TIME_THR[stage][difficulty]
practice: power = stage 0: 0, stage 1: 112, else 128            (checksum)
spell practice: power = id < 2: 30, id < 13: 80, else 128       (checksum)
replay playback: rank init; Replay_Register(1) (loads stage record, sets seed); checksum (RNG preserved)
stage_seed = rng.s
register: Stage(stage), Bullets("etama.anm"), Enemies, Effects, HUD, Gui; Replay_Register(0) if recording
    # order VERIFIED (`GameManager_StageStart` 0x43ABD7): each on-register callback runs inside its FuncChain_AddCalc; Effects'
    # (manager clear, eff anm load, no RNG) and HUD's (Hud_Load: ANM loads + script starts, Msg_Load; no effect spawn)
    # do not touch each other's state, and the calc/draw priorities (13/12 vs 15/17) are distinct, so swapping them
    # changes no logic or RNG (only the ANM load order). RE-Q24
spell practice in 6A/6B/Ex outside the last-word ranges: stage overlay base (0x4E4B64) = 2
unless reloading the same stage (0x17CE8C4): load (not play: `Music_Play` = sound command 1, 02 §4) BGM slot 0 and
    slots 1/2 whose name does not start with ' ' from the STD names (spell practice: slot 0 only,
    BGM chosen from table 0x4C7670 by spell id)
flags |= 4; game_over = 0; ... freeze byte 0x160F534 (GM+0x2C) := 0; stats.score (+0x08) := 0; flags &= ~0x10
    # score := 0 is undone by the first gameplay calc: §4 step 9 sets score := score_display (equal to the score at
    # a clear, §4 step 1; = continues after a continue). VERIFIED 0x43B7FE–0x43B808, RE-Q24
wait (thread) until loading_frames >= 30 (80 in replay playback)      # see 'Loading frame count' below
end loading screen; wait until flags bits 5–6 == 0     # after a clear: until the player confirms (§4)
loading = 0; flags &= ~0x200; HUD repaint 60 frames
```
`TIME_THR` (0x4C77F0, int[stage][4], by difficulty E/N/H/L): st1 2000/2500/2700/3000; st2 6500/7200/7200/7200;
st3 7500/8500/8800/8800; 4A 9999 all; 4B 7500/8500/8500/8500; st5 9999 all; 6A/6B 0. Extra reads past the
table (value 10) — irrelevant because Extra's clock advance is fixed (§4.1).

While `loading ≠ 0`, `GameMgr_Calc` increments `loading_frames` and returns 3 (nothing else in the calc
chain runs); `loading == 2` → return 4 (quit).

**Loading frame count** (changelog/answers_impl17.md §2, VERIFIED code + sessions). The loader thread polls `loading_frames`
with `Sleep(17)`, so after the threshold is reached it ends the loading screen before the next calc in practice:
the stage has **31** loading calcs (81 in replay playback) when the loader's own work is shorter than the threshold.
The loader's work is bounded below by the texture uploads: every `Anm_LoadAndWait` makes the loader sleep until the
main thread has uploaded the file's entries in `Supervisor_Calc` (`AnmManager_PostloadPending`, **one entry per
frame**), plus file decoding time on the loader thread (host dependent). Measured with the session recorder:
fresh start of stage 1 = 48 loading calcs (two sessions, Reimu & Yukari: stg1bg/eff01/enemy/stg1enm/etama/front/
times/stg1txt/player/face files ≈ 41 entries); later stages and demos (fewer files) = 31 / 81. After a stage
clear the loader then also waits for the confirm press (flags bits 5–6), so those rows continue until the press.
Upload timeline (changelog/answers_impl18.md §IMPL19-Q4, VERIFIED at runtime, selftest1 stage 1): `Anm_LoadAndWait` preloads
(reads + decodes) a file on the loader thread, then sets the slot's pending index to 1; each main-thread
`Supervisor_Calc` uploads exactly one entry (`Anm_PostloadStep`); after the last entry the loader preloads the
next file. A file costs `entries` frames, plus an **empty frame for every frame boundary the next file's preload
spans** — 0 for small files, 1–2 for large images (selftest1: stg1bg 1, face_rm00 2, face_yk00 2, face_st01 1 =
6 gaps on top of 42 entries → 48). So the "+6" is host decode time, not one frame per file; the rest of the
stage start (ECL/MSG/STD, gauge setup, `Loading_Wait`) finished within the frame after the last upload.
A port should use max(31 | 81, entries_to_upload + host-dependent gaps; 6 reproduces the recorded fresh stage-1
starts of Reimu & Yukari) and, for session comparison, the recorded `release stage poll`.

**Per-frame depth clear.** `GameMgr_Calc` itself issues `Clear(Z only, 1.0)` on the device's current viewport
(left full-screen 0,0,640,480 by the previous frame's draw — every logged frame shows `C z` with V 0 0 640 480) on
every calc that passes the loading test (loading = 0), after the attract-demo end check and before the pause /
game-over / slow-mode early returns: it runs also while paused and on the game-over menu. It is skipped on: loading
calcs, the stage-end early returns of §4 step 1 (return 3), the attract-demo end frames, and replay "slowdown
reproduction" skipped frames (playback speed byte 1). The colour+depth clear of the playfield comes from
`Stage_DrawLow` (draw prio 6).

### 3.2 Fresh start (new game, and every start in practice / spell practice / Extra)
Condition: `fresh_start || spell practice || stage practice || difficulty ≥ 4`.
```
re-allocate config copy and stats block; heap jitter (RNG 2 draws)
decoys init 0x43B984 (RNG 60 draws)
config copy = current config (60 bytes)
stats.(+0xB0) = 0; stats.gauge (+0x22) = 0
stats.clock (+0x28) = stage == 8 (Extra) ? 6 : 0
config.lives (+0x1C) = 2 if difficulty ≥ 4; 8 if stage practice; else the option value
Player_Register(0)
if not replay playback: stats.lives = config.lives (checksum); bombs = trunc(human .sht header +4) (= 3)
stats.power = 0 (checksum)
0x164D30C = 0 (play time); 0x164D0AC = 0x164D0A8 = 0
score_display = score = score_step = 0; hiscore = 100000 (then from score.dat, §3.4)
stats.continues (+0x29) = 0; stats.total_graze (+0x0C) = 0; stats.point_items_total (+0x30) = 0
config.slow_mode (+0x25) = 0 if difficulty ≥ 4, practice or spell practice
stats.max_point_value (+0x24) = E 60000, N 100000, H 200000, L 300000, Extra 300000
stats.extends (+0x34) = 0; next_extend (+0x38) = EXTEND_THR (08 §8)
GameMgr_LoadScoreData 0x43BBE1: first flushes score.dat via Result_Register(2) = save (RNG 2 draws, 15 §2.2); then load score.dat → hiscore for (shot, difficulty) (practice: practice hiscore table; play counters +1)
rank = RANK[d].init, min, max (table 0x4C7880: E/N 10, 8, 16; H/L 8, 8, 12; Extra 16, 15, 16)
  (`GameMgr_InitRank` 0x43B936 writes rank, min, max only — **not** the sub-counter, which §3.1 zeroes at every stage start)
  (rank changes: RankAdd/RankSub 0x43BFC3/0x43C03F via sub-counter GM+0x3DE38, ±1 rank per 100,
   clamped — RankAdd only to max, RankSub only to min (VERIFIED RE-Q29; the same as a two-sided clamp while rank is in range); deltas: power item +1, point item +3 if v < max else +10, bomb item +5, graze +6, extend +200, item fell off
   the bottom −3, bomb −200, death −1600; +100 every 2400 − 240·lives frames of the enemy-manager clock outside
   dialogue, 0x42C3B0 — called at the **start** of `EnemyMgr_Calc` (inside the bit-10 gate, right after the op-176
   box, before the timelines and the slot loop: enemies firing in that frame already see the new rank; 07 §10.1
   step 1; the test is enemy-manager timer +0x9DCED0 advanced this frame and timer % period == 0; VERIFIED call
   0x42C77E, RE-Q30, fullrun_noclip poll 51632))
stats.deaths (+0x64/+0x68) = 0 (checksum); stats.bombs_used (+0x84/+0x88) = 0 (checksum)
stats.spells_captured (+0x1C) = 0; 0x164D318/320/324 = 0
play counters in the score data incremented unless replay / spell practice / slow mode
if the requested scene was 10 (retry): two more score-data counters +1 (0x164CD6C + 0x44·difficulty and 0x164CF04,
  0x43BE09; PLST [13] retries, 15 §2.9, VERIFIED)
```

### 3.3 Continuing to the next stage (fresh_start = 0, normal difficulty, not practice)
```
flags &= ~0x400; score_display = score; score_step = 0
stats.deaths decoys (+0x68) = 0 (checksum); stats.bombs_used (+0x88) = 0 (checksum)
Player_Register(0)                      # lives, bombs, power, score carry over
```

### 3.4 Replay playback stage load (0x452D60) — VERIFIED
From the replay header: shot (+0x6A), difficulty (+0x6B), config copy (+0xB4, 60 bytes: **all 60 bytes** are copied over the gameplay config copy `*(GM+4)` as stored — no field
selection and no clamping, VERIFIED 0x452D60, RE-Q25). From the stage record
(offset table header+0x20[stage]): point items (+4 → stats+0x30), rank byte (0x164D334), lives, bombs, power
(via the setters), graze total, input stream pointer, 0x164D0B0, max point value, seed, spells captured,
extends, next extend, gauge, clock. Missing record for the requested stage → load fails.
**Score** (changelog/answers_impl14.md §6, VERIFIED): the stage's own +0 is NOT used (it is the end-of-stage score). Instead
score (stats+8) and displayed score (stats+0) := +0 of the **last existing record of a lower stage index**
(scan k = 0 .. stage−1, keep the last non-null record pointer). No earlier record (first recorded stage) → score
is not touched and keeps the fresh-start value 0 (StageStart fresh branch zeroes stats+0/+8).

## 4. In-game flow (`GameMgr_Calc` 0x439BC7, prio 2) — VERIFIED

Per frame (after `0x164D09C += 1`):

1. **Stage-end handling** (flags bits 5–6 ≠ 0):
   - state 2 (clear triggered) → set state 3, **loading (GM+0x38) := 1**, `next_scene_after = −1`, then:
     - not replay: if `continues == 0` set clear bit `1 << stage` in the no-continue mask
       (`gm+0x3C49C + shot*0x24 + diff*2`, also 0x164BB54 + diff*2); always set it in the any-clear mask
       (`gm+0x3C4A6 …`, 0x164BB5E …).
     - score_display (stats+0) := score (stats+8), for **every** stage (VERIFIED 0x439BC7, RE-Q24).
     - stage practice → score_display = score again; `next_scene_after = 6` (result); return 3.
     - stage 6A/6B/Extra: replay → `next_scene_after = 7`. Not replay: Extra (diff ≥ 4) → Extra-clear bit
       0x8000, clear count++, flags |= 0x10, `next_scene_after = 6`, return 3; else flags |= 0x10,
       `next_scene_after = 9` (ending), return 3.
     - stage 1–5, not replay: if `clock > 11` → `next_scene_after = 9` (ending), flags &= ~0x10, return 3
       (night ran out); else `Stage_Advance` (0x43C4B3) and `scene = 3` (reload GM for the next stage).
     - stage 1–5, replay: next stage = first later stage that has a record → `scene = 3`, none → `scene = 7`.
     - if `next_scene_after < 0`: `Hud_StageTransition` 0x438F58: capture.anm VM impl+0x3CC0 := script 1 and a playfield
       capture into its sprite rect (12 §3), applied after this frame's Present (VERIFIED RE-Q34).
   - Any state ≠ 0: if (SHOT|ENTER newly pressed on the raw input) or replay playback or stage ≥ 6:
     clear bits 5–6 and, if `next_scene_after ≥ 0`, `scene = next_scene_after`.
     (So after a stage-1–5 clear, the next stage's loader thread waits on the clear screen until the player
     confirms; endings/results after the final stage happen immediately.)
   - **Clear-frame timing** (VERIFIED, RE-Q24): the "return 3" exits above skip the confirm test on the clear frame,
     so for practice, 6A/6B/Extra (not replay) and the night-over ending `scene = next_scene_after` is written on the
     **next** frame (state 3, stage ≥ 6 or replay → no key needed; practice/night-over in stages 1–5 wait for a newly
     pressed SHOT/ENTER). The paths that do not return (stage 1–5 advance, replay) run the confirm test on the clear
     frame itself. Because loading was set to 1, every path then takes the loading branch of step 2 (loading_frames
     += 1, return 3) on this and all later frames until a loader clears it: no gameplay calc after the clear frame.
2. If loading → §3.1 (return 3 or 4).
3. Deferred music start (`gm+0x3DE28`).
4. **Pause**: if not paused, no game-over menu, not demo, `gm+0x2D == 0`, and ESC newly pressed (raw input):
   paused = 1, sound command "Pause", sfx 0x22 (`se_pause`), music paused, pauses++, checksum with RNG saved
   and restored.
5. Playfield viewport set; replay-playback frame skipping (VERIFIED 0x43A24A, RE-Q29). Only when replay playback
   (flags bit 3), playback speed byte 0x164D0BD = 1 **and** no dialogue is active (`Msg_IsActive` 0x4358BB = 0):
   the shared skip counter `c` = GM+0x3DE08 += 1, then with `f` = fps_now (0x17CE8F0, from the replay's fps stream,
   15 §3.3) the frame runs only if all of: (f ≥ 20 or c % 3 = 0), (f < 20 or f ≥ 30 or c % 2 = 0),
   (f < 30 or f ≥ 40 or c % 3 ≠ 0), (f < 40 or f ≥ 50 or c % 6 ≠ 0); otherwise **return 3** at once (steps 6–11
   skipped, frame counter not advanced, `gm+0x2D` unchanged). Purpose (INFERRED): emulate the recorded slowdown.
6. Demo mode: any newly pressed key → `scene = 1`; demo timer limits per demo index 0–3: music fade +
   ScreenEffect type 2 at 6000/4800/4920/6900 frames, `scene = 1` and return 3 at 6120/4920/5040/7020.
   Exact calls (0x43A3FA, VERIFIED): `ScreenEffect_Register(type 2, p0 = 120 frames, colour 0 = black, 0, 0,
   draw prio 0x15)` (fade the playfield to black over 120 frames) then `Music_Fade(3.0 s)` (WAV mode: ÷ game
   speed when 0 < speed ≤ 1; MIDI: Midi_Fade(3)). Demo index GM+0x3DBB4 (0x164D0BC): the GameManager static
   constructor 0x43C5E1 sets it to **3**; `MainMenu_Top` does `n = (n + 1) % 4` before loading
   `demorpy<n>.rpy`, so the first attract demo after boot is demorpy0, then 1, 2, 3, 0 …
7. Anti-tamper checks of the decoy values (only affect a debug float 0x164D2FC; not needed). In the middle of them
   (VERIFIED RE-Q27, store at 0x43A5BC): **GM flags bit 2 := (paused byte == 0 and game-over byte == 0)**, so on the ESC frame
   (step 4 has just set paused = 1) bit 2 is already 0 when step 8 returns (selftest1 row 1423: flags 0x10000). `GameMgr_IsTampered` 0x40BB80 has more callers (bomb input, AddLives/AddPower/AddBombs, EffectMgr quit); all are inert
   in an untampered run, which is the only case a faithful port has to model (09 §6.1).
8. If paused (1 or 2) or game-over menu open → return 3 (calc chain stops: the whole game is frozen; the
   ASCII manager at prio 1 runs the pause / continue menus, 12).
9. Score display roll (score cap first; the rest only when disp ≠ score): **if score < disp: score := disp** (this
   is how the score survives `StageStart`'s score := 0, §3.1; VERIFIED RE-Q24); `d = clamp((score − disp) >> 5, 1, 578910)` (unsigned); `step = max(step, d)`; if `disp + step >
   score` then `step = score − disp`; `disp += step`; when `disp ≥ score`: `disp = score, step = 0`; score
   capped at 999 999 999 (×10 displayed). `hiscore = max(hiscore, disp)`; on update hiscore-continues byte
   (+0x18) = continues.
10. Slow mode (config +0x25 ≠ 0; VERIFIED 0x43A92D–0x43A9DC, RE-Q29). Runs after step 9 (the score roll and the anti-tamper
    checks have already run this frame). `gm+0x2D` (0x160F535) := 0; `c` = GM+0x3DE08 += 1; with `n` = live-bullet
    count 0x160F3C8 (08 §4.3a, from the last bullet calc): if (n ≥ 320 and c % 3 = 0) or (224 ≤ n < 320 and
    c % 4 = 0) or (128 ≤ n < 224 and c % 5 = 0) → `gm+0x2D` := 1 and **return 3** (step 11 and the
    `GameMgr_IsTampered` call skipped; frame_counter not advanced). Otherwise, if n < 128: `c` := 0.
    - `c` is the same counter step 5 uses (both only increment it; step 5 never resets it). Its only other writer is
      the GM static constructor (zero), so it is **never reset** by game start, stage start or GM removal: its phase
      carries over between games, between replay playback and live play (VERIFIED: the 12 byte-pattern sites of offset
      0x3DE08 are all in `GameMgr_Calc`; the only struct-base fill covering it is the constructor's).
    - `gm+0x2D` is cleared only here (every frame that reaches step 10 with slow mode on), by `GameManager_StageStart`
      (0x43B835) and by `GameMgr_OnRemove` (0x43BF5C). A frame that returns before step 10 (loading, pause, game-over
      menu, playback skip of step 5) leaves it as it is. Readers: step 4 (through `this`), `Supervisor_Calc` input
      poll (§7) and `Fps_Update` 0x446F53 (a flagged frame is not counted by the fps meter).
    - Because the Supervisor (prio 0) runs before the GM (prio 2), a skip on frame k makes the poll of frame k+1
      OR-accumulate (§7), and step 4 of frame k+1 sees `gm+0x2D` = 1, so ESC cannot pause on the frame right after a
      skip.
    - In slow mode the recorder stores nothing (15 §3.2 step 2) and playback reads nothing (15 §3.3), so slow mode
      changes only the live game's speed.
11. `frame_counter += 1`; return 1.

### 4.1 Stage routing (`Stage_Advance` 0x43C4B3) — VERIFIED
| from | to |
|---|---|
| 0 (St1) | 1 |
| 1 (St2) | 2 |
| 2 (St3) | 3 (4A) for teams 1, 2 and solos 6–9; 4 (4B) for teams 0, 3 and solos 4, 5, 10, 11 |
| 3, 4 (4A/4B) | 5 |
| 5 (St5) | 6 (6A) if route bits 11–12 == 0, else 7 (6B) |
| 6 | 7 (unreachable in normal play: 6A ends the game) |

The final route is decided by the stage-5 post-boss dialogue (10 §4: forced A, forced B, or the choice).

Clock advance at a stage clear (`GameMgr_ClockAdvance` 0x43C35F, applied by msg op 9): stages 1–5:
`+2 if time < time_threshold else +1`; 6A/6B: +0; Extra: +4. The HUD animates the clock display and then
sets flags bits 5–6 = 2 (stages 1–5); stages 6A/6B/Extra are triggered by msg op 11 instead.

### 4.2 Death, game over, continue — VERIFIED
- Respawn after a death (`Player_DyingUpdate` 0x44CBF0): if lives > 0: `AddLives(−1)` (checksum),
  bombs = trunc(human sht +4); else `game_over = 1` (GM+0x3DBB3). Timeline op 16 also sets `game_over = 1`.
- Game-over menu (ASCII manager, 0x404890), first frame:
  - stage practice (not spell practice) → `scene = 6` (result); replay → `scene = 7`.
  - normal difficulty, not spell practice, and (`clock > 10` or stage == 6B): no continue →
    flags &= ~0x10, `scene = 9` (ending). (Extra/other → `scene = 6`.)
  - otherwise the continue prompt (Yes/No, 12). "No" → after 20 frames `scene = 6` (result).
    "Yes" in spell practice / Extra → retry: `scene = 11`. "Yes" in a normal game → continue sequence:
    frame 15: clock += 1 (HUD clock sprite updated); frame 60: interrupts; frame 90: menu closes,
    `continues += 1`, **score = score_display = continues** (so the last digit shows the continue count),
    graze_stage = 0, lives = config.lives (checksum), bombs = trunc(human sht +4), point items (stage and
    total) = 0, power = 0 (checksum), extends = 0, next_extend = 100, HUD repaint, sound resumes.
- Pause menu (12): resume, "quit" → Result_Register(2) (save) and `scene = 1`; "retry" → spell practice /
  stage practice / Extra: `scene = 11` (restart this stage, keep loaded data); otherwise `scene = 10`
  (restart from stage 1).

## 5. Stats block (`*(GM+8)`, 0xE4 bytes)

| off | type | meaning | conf |
|---|---|---|---|
| 0x00 | u32 | displayed score (÷10 units) | VERIFIED |
| 0x04 | i32 | graze this stage (cap 99 999) | VERIFIED |
| 0x08 | u32 | score (÷10 units, cap 999 999 999) | VERIFIED |
| 0x0C | i32 | graze total (cap 999 999) | VERIFIED |
| 0x10 | u32 | score-roll step | VERIFIED |
| 0x14 | u32 | high score | VERIFIED |
| 0x18 | u8 | continues shown next to the high score: set by `GameMgr_LoadScoreData` (`ScoreDat_GetHiscore` param 4 = continues byte +0x26 of the top HSCR entry of (shot, difficulty), 0 when the file has none; also in practice, where only +0x14 is replaced; VERIFIED 0x43BBE1 / 0x45A950, RE-Q32), then by the score roll when the displayed score passes the hiscore (§2 step 9) | VERIFIED |
| 0x1C | i32 | spell cards captured | VERIFIED (+1 per capture) |
| 0x20 | i16 | gauge at stage start | VERIFIED |
| 0x22 | i16 | human/youkai gauge (09) | VERIFIED |
| 0x24 | i32 | max point value (point-item base) | VERIFIED |
| 0x28 | i8 | clock / night counter (0–12; HUD `times.anm` sprite) | VERIFIED |
| 0x29 | u8 | continues used | VERIFIED |
| 0x2C | i32 | point items this stage | VERIFIED |
| 0x30 | i32 | point items total (extend counter) | VERIFIED |
| 0x34 | i32 | extends obtained (−1 = disabled, stages 6A/6B/Ex after op 9) | VERIFIED |
| 0x38 | i32 | next extend threshold | VERIFIED |
| 0x3C | i32 | time this stage. Reset to 0 at every stage start (§3.1). `GameMgr_AddTime` 0x418220 (n < 0 and time < −n → 0, else time += n, +0x44 += n, GM+0x3DAAC += n, checksum). Writers: +1 per collected time orb; card capture reward (Spell_End stores it in g_Gui +0x10, paid by `Spell_Calc` 130 frames later or by the next `Spell_Start`); −15 per frame of the death-bomb window (`Player_DyingUpdate`); at the death itself −time/10 if time ≤ 5000, else −500 (changelog/answers_impl20.md §Q1) | VERIFIED |
| 0x40 | i32 | time threshold for the clock advance | VERIFIED |
| 0x44 | i32 | time parity accumulator: += n on every non-clamped `GameMgr_AddTime`, including negative n (death-bomb drain, death penalty); only bit 0 is read (max-point step, §1.5 setters) | VERIFIED |
| 0x48..0x60 | i32×7 | decoys (random 6543..106542) | VERIFIED |
| 0x50 | i32 | checksum random A (inside the decoy range) | VERIFIED |
| 0x64, 0x68 | f32 | deaths (pair) | VERIFIED |
| 0x6C..0x70 | f32×2 | decoy floats | VERIFIED |
| 0x74 | f32 | lives | VERIFIED |
| 0x78..0x7C | f32×2 | decoy floats | VERIFIED |
| 0x80 | f32 | bombs | VERIFIED |
| 0x84, 0x88 | f32 | bombs used (pair) | VERIFIED |
| 0x8C..0x94 | f32×3 | decoy floats | VERIFIED |
| 0x98 | f32 | power (0–128) | VERIFIED |
| 0x9C..0xA0 | f32×2 | decoy floats | VERIFIED |
| 0xA4, 0xA8..0xC4 | i32 | decoys | VERIFIED |
| 0xB4 | i32 | checksum random B | VERIFIED |
| 0xC8 | i32 | copy of 0x50 | VERIFIED |
| 0xCC | i32 | checksum (sum over decoys, config, GM block) | VERIFIED |
| 0xD0..0xE0 | i32×5 | decoys | VERIFIED |

Getters truncate the floats (`trunc`). Setters/adders: SetLives 0x406E20, AddLives 0x43C641, SetBombs
0x406F30 (no RNG), AddBombs 0x439883, SetPower 0x406FA0, AddPower 0x441850, AddTime 0x418220 (if n < 0 and time < −n: time := 0 and nothing else — no parity, no
checksum; otherwise time += n, **time_parity +0x44 += n for every n, negative ones included**, GM+0x3DAAC += n,
checksum (4 draws), and for n > 0 `max_point_value += ((n + (time_parity & 1)) / 2) · 10` using the updated parity.
So the −15 per frame of a death-bomb window and the death penalty change the parity too: attract demo 1 frames
3844–3854 (11 × −15) flip it, and the point items at 3870 (and demo 2 at 3082) depend on it; VERIFIED 0x418220, RE-Q27), AddScore 0x4181F0
(`score += n/10`), Extend 0x439B29 (lives < 8 → +1 life else bombs < 8 → +1 bomb; sfx 0x1C; rank +200).
A reimplementation can drop all decoys/checksums **but must still consume the RNG** exactly where
`UpdateChecksum` / `ScrambleDecoys` run.

## 6. Replay stage record (0x24-byte header per stage) — VERIFIED
| off | type | field |
|---|---|---|
| 0x00 | u32 | score ÷10 at the **end** of this stage: filled in when the next stage starts (or at replay finish for the last stage). Corrected in Phase 6, see 15 §3.2 |
| 0x04 | i32 | `stats+0x30` point items total |
| 0x08 | u32 | `stats+0x0C` graze total |
| 0x0C | i32 | extends |
| 0x10 | i32 | next extend |
| 0x14 | i32 | max point value |
| 0x18 | i16 | gauge |
| 0x1A | u16 | RNG seed |
| 0x1C | u8 | power |
| 0x1D | u8 | lives |
| 0x1E | u8 | bombs |
| 0x1F | u8 | rank byte 0x164D334 |
| 0x20 | u8 | 0x164D0B0 |
| 0x21 | u8 | spells captured |
| 0x22 | i8 | clock |
| 0x24.. | u16[] | input words, one per gameplay frame (recorded only while not paused / not in the game-over menu; during the stage-clear bonus at most 3 frames) |

A second per-stage stream holds one byte per 30 frames: FPS value (bit 7 = flag).

## 7. Supervisor scenes (`Supervisor_Calc` 0x445453, calc prio 0) — VERIFIED

Fields: `+0x158` current scene, `+0x15C` requested scene (0x17CE8B4, written by everyone), `+0x160` previous.
Every frame (before the scene logic): reset some ANM manager state, run Supervisor VMs, poll input (raw word
0x164D528, previous 0x164D530, key-repeat flag 0x164D538), unless `gm+0x2D` (slow-mode skipped frame, §4 step 10), in which
case input is OR-accumulated: raw word |= Input_Poll(), and **nothing else** changes: previous word, repeat flag
0x164D538 (keeps its value from the last normal poll) and repeat counter 0x164D53C are left as they are (VERIFIED
0x4455D5, RE-Q29). Key repeat (VERIFIED 0x445453, the same rule as the replay emulation
15 §3.3): repeat := 0; if the new raw word equals the previous one: **test first** — if the counter 0x164D53C > 29,
repeat := (counter % 8 == 0), and if the counter > 37 it is reset to 30 — **then** counter += 1; else counter := 0.
The tested value runs 0, 1, …, 32 (repeat), 33 … 38 (reset to 30, stored 31), 31, 32 (repeat), …, so repeat is set
on the 33rd, 41st, 49th, … consecutive unchanged frame (every 8 frames), never on the 32nd.

When `requested ≠ current`: `previous = current`, then by `(current, requested)`:

| current | requested | action |
|---|---|---|
| 0 (boot) | any | `requested = 1`; register MainMenu |
| 1 menu | −1 | quit (return 4) |
| 1 | 2 | register GM (new game / practice / replay as set up by the menu) |
| 1 | 4 | return 5 (exit with error) |
| 1 | 5 | `Result_Register(0)` (score viewer) |
| 1 | 8 | register MusicRoom |
| 1 | 9 | unregister GM; register Ending |
| 2 game | −1 | quit |
| 2 | 1 | unregister GM; replay finish (0x4531F0); `requested = 1`; register MainMenu |
| 2 | 3 | unregister GM; register GM; `requested = 2` (next stage) |
| 2 | 6 | unregister GM; `Result_Register(1)` (name entry) |
| 2 | 7 | unregister GM; replay finish; `requested = 1`; release managed textures; register MainMenu |
| 2 | 9 | unregister GM; register Ending |
| 2 | 10 | unregister GM; if not practice and difficulty < 4: stage = 0; replay finish; register GM; `requested = 2` (retry from the start) |
| 2 | 11 | `requested = 3`, reload-same-stage flag (0x17CE8C4) = 1; unregister GM; register GM; `requested = 2` |
| 2 | 12 | `requested = 3`; unregister GM; `Stage_Advance`; register GM; `requested = 2` (never requested in the code) |
| 5 result | −1 / 1 | quit / register MainMenu |
| 6 result (after a game) | −1 | replay finish, quit |
| 6 | 1 | replay finish; register MainMenu |
| 8 music room | −1 / 1 | quit / MainMenu |
| 9 ending | −1 / 1 | quit / MainMenu |
| 9 | 6 | `Result_Register(1)` |

Afterwards the raw input words are cleared (so the new scene sees no "held" keys), `current = requested`,
the frame counter `+0x154` increments (every 4000 frames a periodic check of the config file may quit),
and the "frames since load" counter 0x18B8A2C decrements.
Unregistering the GM (0x43BF6E) also clamps the score to 999 999 999, sets displayed score = score and resets
game speed to 1. The GM on-remove callback stops music (unless spell practice reload), waits for sounds,
unregisters Gui, Stage, HUD, Player, Enemies, Effects, Bullets, Replay (recording) and saves score data.

Exit / fresh-start flags (VERIFIED, changelog/answers_impl13.md §2): `GameMgr_OnRemove` first sets the full-exit flag
0x17CE8C0 = (requested ∉ {3, 11, 12}). `GameMgr_OnRegister` first sets the fresh-start flag 0x17CE8BC with
the same test. The supervisor writes requested = 3 before it unregisters for 11 and 12, so those two look like
"next stage" to the GM. A retry (10) is a full exit **and** a fresh start. A full exit hides the gauge readout
(12 §5.9), stops the music even in spell practice, and makes `Player_OnRemove` release the player's sht files.

Requested-scene writers: menu (2 start game, 5 result, 8 music room, −1 quit), GM calc (3, 6, 7, 9, 1),
pause menu (1, 10, 11), continue menu (6, 7, 9, 11), demo (1), result screen (1), ending (6), staff/ending
(0x429D40: 6).

## 8. Time / night system summary
- `stats.time` (+0x3C) gains +1 per time item, + capture time bonus at spell end (100…1000, 700 for fixed
  bonus cards; see below); −15 per frame while the death-bomb window counts down (0x44CBF0) and
  −min(time/10, 500) when the death completes; AddTime clamps at 0.
- At the stage clear the clock (`stats.clock`) advances (§4.1); after stage 1–5 clears `clock > 11` ends the
  game with the "night over" ending path; the continue menu refuses continues at `clock > 10` (and in 6B),
  each continue costs +1 clock. Extra starts at clock 6. ECL op 181 adds +1 (max 12) during play.
- Spell capture time bonus (spell end 0x4161B0): fixed-bonus card → 700; otherwise with `T` = total spell
  time and `r` = remaining time: `thr = T − T/7`; `r ≥ thr` → 1000; `r < 180` → 100; else
  `(r − 180)·900/(thr − 180) + 100`. Awarded with the score bonus when the capture display finishes; capture
  also increments spells captured and runs a checksum update (RNG).
