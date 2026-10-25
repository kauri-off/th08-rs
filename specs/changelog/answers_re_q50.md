# RE-Q50 — replay pilot: spell-practice and practice divergences

Code: decompile of th08.exe (`EnemyManager_Load` 0x42EBF0, `Stage_Load`, `GameMgr_IsSpellPractice` 0x406C70,
`EclNative_19` 0x424FC0, `GameManager_StageStart` 0x43ABD7, `ShotInit_Laser` 0x44FE20, `Player_Load` 0x44D650,
`Sht_Load` 0x44DD70, `FileSystem_OpenFile` 0x43E660). Name tables read from the exe (0x4C731C/0x4C7340, 0x4C73CC/0x4C73F0,
0x4C7364, card tables 0x4C70E4 / 0x4C7054). ECL data: `ecl_dis.txt` timelines of all `*sp` / `_xx` files.
Traces used only to locate: pilot sessions th08_ext_replays/ref/*, pilot port traces (40cc7d4); one original re-run with
TH08_SCAN (re/work/runs/q50/orig_52104/scan.txt, polls 600–660). Confirmation runs with a scratch copy of the port
(re/work/runs/q50/portcopy, target/re50; src/ untouched): port_*_fix.

| # | question | original | spec | port |
|---|---|---|---|---|
| 1 | Bu08 / JS06 (spell practice 6B No.171, Extra No.192): `rng` + enemies 1\|0 at gameplay frame 2 | Spell practice is selected by GameFlags bit 14 only; g_StageIdx stays 0–8 and indexes the second half of the name tables: `ecldataNsp.ecl` (0x4C73F0), `stageN_s.std` (0x4C7340); card ≥ 205 → ECL / enemy ANM from the card-indexed tables 0x4C70E4 / 0x4C7054 (205–215 stage `sp` files, 216–221 `ecldata_xx` / `stgenm_xx`). Every `sp` timeline spawns an effect enemy at time 1 (→ enemy at frame 2, its effects draw RNG) and the boss at time 30. The port's first spawns at frames 261 / 401 are exactly `ecldata7.ecl` / `ecldata8.ecl` time 260 / 400: it ran the normal stage files | 07 §1 (exact selection table, card table contents, sp timeline shape — new), 04 (rows `stageN_s.std`, `stgenm_xx`, `ecldataNsp`, `ecldata_xx`, stage index note — corrected: "stage index 9–17" was wrong), 06 §2 (bit-14 test) | CODE BUG A |
| 2 | Nd115 (spell practice stage 3 No.54): no boss at frame 31 | Same cause: `ecldata3.ecl` also spawns at time 1 (sub19), so frame 2 matched, but its boss comes much later; `ecldata3sp` spawns the boss sub (sub63) at time 30. The boss sub's first instruction is native 19, which sets int var 10000 := i16 0x164D0B8, the **spell-practice card id** (global from the card list / replay header), and `ijg $10000, …` chains pick the card. Not the running Spell object's id (none is active at that time) | 07 §12 row 19 (corrected: "current spell id" was ambiguous) | CODE BUG A + B |
| 3 | next earliest divergence elsewhere: silentselene 52104 (Alice solo, shot 7, power 0) stage 1 frame 432 `score` 1\|0 (51852 = RE-Q49 bug 1, skipped) | `ShotInit_Laser` compares the slot's entry **pointer** (player +0xE2A80[slot]) with the entry being fired. `Player_Load` loads the human and youkai sht with two `Sht_Load` calls, each `FileSystem_OpenFile` decoding into its own buffer, so a solo's focused and unfocused entries (same file `ply01as.sht`) are different objects: a focus change ends the laser (interrupt 1, slot cleared, no bullet that frame) and the next firing frame creates a new one with timer 0. In 52104 focus is released at frame 403 → new laser, timer 0 at 404; the laser (type 5) hits only on even timer values (09 §4.5): t 28 at 432 (life 20 → 13, score +1), t 30 at 434, plus a 1-damage trail rect at 435; port laser timer 265 at 432 (odd) → hits shifted by one frame | 14 §4 (pointer comparison, focus change ends the laser — new), 09 §2 (two sht buffers — new), 09 §2.1 init-4 row | CODE BUG C |
| – | (checked while answering 1) spell-practice power at stage start | Both the practice and the spell-practice power assignments are inside the GameFlags bit 0 test; replays play with bit 0 clear, so neither runs | 11 §3.1 (comment added) | right (`gamemgr.rs` uses `practice()`) |

Confirmation (scratch port copy with fixes A–C; src/ untouched):
- 52104: first difference 621 → **690** (`score` 0x7739/0x7738, not analysed).
- Bu08 191 → **4428**, JS06 191 → **914**, Nd115 220 → **1184**: all three now differ only about 32 rows before the end
  (`frame` one less, GameFlags bit 2 clear one frame earlier at the end of the replay; not analysed).

## CODE BUG list
A. `gamemgr.rs` `GameManager::stage_start` → `Stage::register` / `EnemyManager::register` (`stage.rs` `files`,
   `enemy.rs` `files`): spell practice must load `stageN_s.std` and `ecldataNsp.ecl` when GameFlags bit 14 is set, with
   the stage index 0–8 (07 §1 table). Today the `_s` / `sp` names are chosen only for an index ≥ 9, which the game path
   never passes, so every spell practice (played or replayed) runs the normal stage files. Also missing: card ≥ 205 →
   ECL and slot-8 ANM from the card tables (07 §1: 205–215 stage `sp` files / `stgNenm.anm`, 216–221 `ecldata_xx.ecl` /
   `stgenm_xx.anm`). `stgNbg.anm`, `enemy.anm` unchanged.
B. `eclvm.rs` native 19: int var 10000 := `game.card` (i16 0x164D0B8, sign-extended), not the active Spell's id
   (07 §12 row 19).
C. `pshot.rs` shot init 4 (laser, 0x44FE20): "different entry" must be identity, including which sht copy (human
   `sht` / youkai `sht_youkai`) the entry came from; comparing `ShotEntry` values misses the focus change of a solo
   (same file in both copies), so the laser is never restarted (14 §4).
