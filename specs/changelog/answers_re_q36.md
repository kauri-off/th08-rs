# RE-Q36 — SPEC-GAP batch 2: rest of gameplay (2026-10-24)

Brief: docs/briefs/RE_Q36_BRIEF.md; markers: docs/specgaps/batch2_gameplay.txt (23). Runs: re/work/runs/q36/
(CHECKPOINT.md; scenes.py / gf.py = golden draw-log frame scans). Evidence: decompile / disassembly (fn.sh, disx.py),
byte scans of absolute references (q34 tools/calls.py), sht_dump / anm_dis / ecl_dis data scans, the golden reference
draw log (sessions/golden) against the q34 port run (re/work/runs/q34/golden_E). No new session run was needed.

## Changed spec sections
| spec § | change |
|---|---|
| 09 §2.1 init 7 | RNG draw only when the phase test fires (after `Player_InitShot`) |
| 09 §2.1 update 4 | no shipped entry uses it (unreachable) |
| 09 §6.2 | 0x164D0B0 := 0 at the real death in `Player_DyingUpdate` (not `Player_Die`) |
| 09 §7 | bomb start test is a **held** test on g_Input (no edge); only the special-mode SE uses an edge; special mode (bits 7–8) skips the whole cost block (no bombs, counters, `AddBombCount`, checksum draws) |
| 11 §2 (+0x3DBA8) | all writers / the only reader of 0x164D0B0 |
| 11 §2 (+0x3DE28) | the only writer and its values 1 / 2 |
| 11 §3 | `GameMgr_Draw` 0x43AA03: return 3 on loading / scene ≠ 2 / bits 5–6 == 1 |
| 11 §4 step 3 | the deferred-music block (name plate, surface 8 release, `Music_PlaySlot` track tables) |
| 02 §4 table | 0x4C7670 record fields +4 track, +0x10 restart flag |
| 12 §3 | state 10 music rule; state 0 starts all 10 scripts and hides VM 8 / VM 9 (practice / slow mode) |
| 07 §12 rows 4, 7, 21 | exact warp algorithm and constants (zones, previous position, cooldown +0xDC4, scale, vel/angle reversal, native-7 −31.1 quirk) |
| 08 §2.2 (+0xDC4) | warp cooldown |
| 13 §2.6 (type 50) | 64 written by `Bomb_Dissolve` is never drawn: the update sets 32 first in the same frame |
| 06 §11.1 | the sparkle-point test has no +0xB24 term; it is implied by when `Stage_DrawLayer` runs |
| 05 §5 ops 45/47/55/57 | integer divide by 0 crashes; unreachable (only `idiv3 …, 2`) |
| 10 §6 | text DIB passes: fresh A1R5G5B5 DIB, alpha inversion, 1-bit alpha, vertical colour gradient; D3DX8 is statically linked |
| 15 §2.2 | equivalence note for loading every block at boot |

## Markers
| port marker | answer | spec § | verdict |
|---|---|---|---|
| src/pshot.rs:607 (init 7 RNG) | 0x4501B0: phase test → `Player_InitShot` → `Rng_Signed`, only on a firing frame | 09 §2.1 | right |
| src/pshot.rs:663 (update 4) | used by no entry of the 8 sht files (update ids 0, 1, 3, 5 only) | 09 §2.1 | right (unreachable) |
| src/player.rs:714 (0x164D0B0 at death) | reset in `Player_DyingUpdate` at the real death, also by `Item_CollectPowerSmall` (power < 128) and the fresh start; read only by the replay recorder, so always 0 in a live game | 11 §2, 09 §6.2 | **CODE BUG** (unobservable: wrong place, and the power-item reset is missing) |
| src/bomb.rs:450 (+0xE2A7C in special mode) | already in the spec (RE-Q30): kept. **New**: the start test reads the **held** bomb bit (asm 0x44C85A `movzx [0x164D52C]; and 2`), with no edge test; only the special-mode SE 0x29 uses an edge | 09 §1.1 row, 09 §7 | +0xE2A7C right; **CODE BUG** reachable: port uses `inp.pressed` (edge), so holding bomb does not chain bombs or fire on the first deathbomb frame |
| src/bomb.rs:502 (HSCR +0x78) | 0x164CFAC += 1 after the cost, normal mode only; order immaterial. **New**: in special mode (bits 7–8) the whole cost block is skipped (index 4 only) | 15 §2.3 (0x78), 09 §7 | counter right; **CODE BUG** reachable (death during a boss last spell): the port charges bombs, counts last spell / bomb count and draws 2–4 extra checksum RNG values |
| src/bomb.rs:787 (VM script before RNG point) | already in the spec (RE-Q24): no player*.anm script draws RNG at time 0 | 14 §2.2 | right |
| src/bullet.rs:1241 (warp) | exact algorithm: per bullet cooldown +0xDC4; zone of pos and of pos − vel (inner / outer strict / outside); on a zone change cooldown 2, vel × −1, angle + π, pos scaled about the centre outward (inner crossing) or inward (outer crossing); native 7 previous-pos bound −31.1 | 07 §12 rows 4/7/21, 08 §2.2 | **CODE BUG** reachable (stage 4A boss: ecldata4a sub28/46, 4asp sub13/31) |
| src/game.rs:102 (whole score.dat at boot) | boot takes PLST only, but every in-memory change reaches a save before the next re-take; loading once is equivalent given the slow-mode CATK restore | 15 §2.2 | right (INFERRED equivalence) |
| src/game.rs:152 (SE off and BGM queue) | already in the spec (RE-Q30): the BGM command queue runs before the SE flush, independent of "SE on" | 13 §1.3 | right |
| src/gamemgr.rs:864 (gm+0x3DE28) | set by every `StageStart`, reload-same included: 2 only for a spell-practice reload whose card record flag is 0, else 1; step 3 plays slot 0 on 1 | 11 §2, §4 step 3, 12 §3 | **CODE BUG** reachable: the port never sets it on a reload, so a stage-practice / Extra retry or continue (music stopped by the pause retry) and a th08_13b spell-practice retry have no BGM |
| src/gamemgr.rs:1072 (GM draw on post-clear frames) | `GameMgr_Draw` returns 3 while loading ≠ 0 (post-clear frames included), scene ≠ 2 or bits 5–6 == 1: nothing above draw prio 5 is drawn. Golden present 10917: ref 0 primitives, port 20 | 11 §3 | **CODE BUG** (draw) |
| src/stage.rs:371 (overlay before quads) | already in the spec: no stage-bg / text.anm script 33 draws RNG, VMs independent | 06 §2 step 4 | right |
| src/stage.rs:815 (+0xB24 ≤ 1 for sparkles) | not in the append test, but implied: `Stage_DrawLayer` only runs while +0xB24 < 2 | 06 §11.1, §10.2 | right (redundant) |
| src/effect.rs:467 (dissolve ring 64 vs 32) | the type-50 update writes 32 first in the spawn frame, before any draw; the bit-10 freeze cannot be on | 13 §2.6 | right |
| src/anmvm.rs:912 (int divide by 0) | x86 `idiv` fault (crash); only `idiv3 $I4, 300/50, 2` in the data | 05 §5 (ops 45–58) | right (unreachable) |
| src/item.rs:267 | answered in RE-Q34: the message is outside the spell-card test | 08 §8.5 | right |
| src/hud.rs:755 | answered in RE-Q34: message types 2–4 are never shown | 12 §5.5 | right |
| src/hud.rs:831 (f = 0) | already in the spec: unguarded `idiv`, unreachable | 12 §5.6 | right (unreachable) |
| src/hud.rs:1133 | answered in RE-Q34: +0x21810 = tiles still running | 12 §5.7 | right |
| src/ascii.rs:839 (script 21) | state 0 starts all ten scripts, then hides VM 8 unless practice and VM 9 unless the gameplay config slow byte ≠ 0 | 12 §3 | **CODE BUG** reachable: the port has the Slow option (menu.rs) but never runs script 21 |
| src/ascii.rs:1023 (capture VM) | already in the spec (RE-Q29): script 0, pos (32, 16, 0) | 12 §3 | right |
| src/text.rs:2 (GDI font) | more is specifiable: fresh A1R5G5B5 DIB, alpha inverted before and after `TextOut` → 1-bit alpha, 5-bit colour, then a vertical colour gradient on the opaque pixels. Glyph shapes stay platform | 10 §6 | **CODE BUG** (draw): port has partial alpha, 8-bit colour, no gradient |
| src/text.rs:196 (TRIANGLE filter) | D3DX8 is statically linked (0x47ABAB), so the filter is reversible from the exe; not done (needs its own task) | 10 §6 | SPEC-GAP stays (open) |
