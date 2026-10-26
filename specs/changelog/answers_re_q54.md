# RE-Q54 — replay pilot: next earliest divergences after K–N

Code: decompile/disassembly of th08.exe (functions per row). Traces only to locate. Runs in re/work/runs/q54/
(`orig_*` = original under the probe, play_session.sh --fast, own game/prefix copies game1/2, pfx1/2, Xvfb :51/:52).
Scratch port `portcopy` (q53 portcopy = impl119 + H–N, plus O–AB below; built with target/re54; src/ untouched),
changes in `q54_scratch.diff` (also contains debug dumps, not part of any fix: TH08_Q54B state-5 site log,
TH08_Q54E enemy dump with rank / manager clock / player state timer / boss timer / final pos, Q52SCAN now prints
area bits, bullet angle/speed and the RNG counter — frame numbers repeat per stage, select by `ctr`).
Probe additions (re/probe/session.inc, pre-change copy q54/session.inc.pre_q54): `TH08_SCANECL=1` adds a `C` line per
enemy (main ctx sub / ip / clock, death sub, chain links, child count, ctx ints 10000–07 and 10036–39);
`TH08_SCANRANK=1` (`_ONLY`) adds a `K` line (manager clock 0xF54DF8, rank, sub-counter, min/max).

| # | session / row | original | spec | port |
|---|---|---|---|---|
| O | Dn14 8210 (bullets) | Yukari boundary side rects: s = radius · f32 0x3F3504F2 (not 0.70711); LS spin `Angle_AddNormalize`; side loop wraps a (≥ π → −2π) at the top, rotation = AddNormalize(a, π/2), next a = that rotation (`EffUpd_YukariSquare` 0x4114E0 / `EffUpd_YukariLSSquare` 0x4117B0). Cancel rect 199 was 0x2A ulps longer and rotated unwrapped → a bullet 0.0007 px inside the edge one frame early | 14 §2.13, §2.14 | CODE BUG O |
| P | 50715 8951 (rng +4) | Last-spell bomb with forced or bombs < 2 uses `GameMgr_SetBombs`(0) (no RNG); only 2 bombs → `AddBombs`(−2) (4 draws) (`Player_UpdateBomb` 0x44C650) | 09 §7 | CODE BUG P |
| Q | Dn14 8539 (enemies, rng) | Ops 93/94 do nothing while the spawner's life ≤ 0 (tested before any argument read); ops 90–92 helpers fail likewise before arg reads, SE 0x24 still plays (`Ecl_Run` 0x5A–0x5E, 0x41F110 / 0x41F280). Familiars killed by op 123's KillAll reach `spawn_rel ×2; delete` in the same frame | 07 op 93/94, §10.5 | CODE BUG Q |
| R | 50715 18447 (rng −24) | Yuyuko butterflies at radius ≥ 500: freed **and** both areas off; angle via `Angle_AddNormalize` (`Bomb_Yuyuko` 0x413140 / LS 0x413990). Areas left on push later areas to higher slots → `Player_InBombArea` (first hit) returns item 6, not the time orb's 7 | 14 §2.15 | CODE BUG R |
| S | 51852 67035, 52104 87347, 80406 76752, 80412 79679, 50715 70325 (flags bit 11) | Stage-5 entry 10 in replay playback takes the route from the replay info byte +0x11C; GameFlags bits 11–12 := choice at once (`Msg_Start` 0x43396D) | 10 §4 step 3 (already right) | CODE BUG S |
| T | 50715 90812, 80412 102823, 53373 116756 (boss) | Var 10100 = `Timer_Get`(g_Gui +0x108), remaining card time; set at `Spell_Start`, `Timer_Decrement`ed (game speed) by every `Spell_Calc`, also after the card (0x41FDD0). ecldata7 sub44/45 (7sp 34/35) test it after `spellcard_end` | 07 §6 row 10100 | CODE BUG T |
| U | Dn14 20208, Bi01 (rng) | Aimed-enemy update only for enemies with flag A bit 6, inside step 6's shot branch after the damage calls (EnemyMgr_Calc 0x42D46A). An intangible boss is never offered → Yukari's option drops it | 14 §3.1 | CODE BUG U |
| V | Dn14 22098 (rank) | `EnemyMgr_TimeRank` 0x42C3B0: manager clock +0x9DCED0 is a Timer ticked at game speed; fires when `cur ≠ prev` and `cur % (2400 − 240·lives) == 0`; zero-filled start | 11 §3.2 | CODE BUG V |
| W | 51249 24766 (rng −4) | `ScreenEffect_Register` links at once: after a fast-forward restart (ret 6) the restarted pass already runs an effect registered in the previous pass | 13 §3 | CODE BUG W |
| X | 52220 49637, 82309 57238, 88234 62194 | `EclNative_2` 0x423400: vel.x negated at x ≤ 0 / ≥ 384 and vel.y at y < −64 **without direction tests**; angle = atan2 whenever any branch (gravity included) ran | 07 §12 row 2 | CODE BUG X |
| Y | Bi01 29288 (timeout one frame early) | Game speed is read live by every timer tick of an enemy update: a slowdown started in this frame's `Ecl_Run` already slows the step-7 tick of e+0x2E14 (and `Enemy_Move`) | 07 §5 "game speed is read live" | CODE BUG Y |
| Z | 53373 116709 (rng) | `Player_UpdateInvuln` 0x44D2C0: state 3 → 0 does `Timer_Set`(+0xE2AF4, 0) (fraction cleared); `Player_DamageAgainst` is gated by that timer changing | 09 §6.4 (already right) | CODE BUG Z |
| AA | 52089 49845 (rng +4) | Shake types 1/7 test GameFlags bit 10 / freeze byte live (0x45BDC0 / 0x45BF10): a spell-card death by a bullet (after `Player_Calc`) freezes the shake from the next calc on | 13 §3 type 7 | CODE BUG AA |
| AB | 51842 69097 (bullets) | `BulletMgr_PlaceBullet` 0x42F5F0 angle order: modes 0/1 (k + aim) + a1; mode 2 ((j·a2 + a1) + i·2π/c1) + aim; mode 4 ((i·2π/c1 + π/c1) + aim) + a1. Port's (k + a1) + aim was 1 ulp off | 07 §9.3 table | CODE BUG AB |

Confirmation (scratch port with H–AB, re/work/runs/q54/fixAB): **all 22 pilot sessions 0 differing rows**; sessions
golden, fullrun_noclip, noclip_test1, options_cfg, selftest1 0 diffs (reg/); gameover_save (row 4215) and
musicroom_practice (row 3506) differ identically with the q53 build and target/release (pre-existing, not analysed).
Per fix: O Dn14 → 8539; P 50715 → 18447; Q Dn14 → 20208; R 50715 → 70325, 51249 → 24766; S 51852 / 52104 / 80406
→ 0, 50715 → 90812, 80412 → 102823; T 50715 / 80412 → 0; U Dn14 → 22098, Bi01 → 29288; V Dn14 → 0; W 51249 → 0,
53373 → 116709; X 52220 / 82309 / 88234 → 0; Y Bi01 → 0; Z 53373 → 116756, T (game-speed decrement) → 0; AA 52089
→ 0; AB 51842 → 0.

## CODE BUG list
O. `effect.rs` `boundary_sides` + kind 37 update (14 §2.13/§2.14): s = radius · f32::from_bits(0x3F3504F2);
   per side `if a >= PI { a -= 2π }`, rot = `wrap_add(a, π/2)`, next a = rot; LS spin `wrap_add(start, ±π/80)`.
P. `bomb.rs` `start_bomb` (09 §7): last spell with forced or bombs < 2 → bombs := 0 without RNG (SetBombs);
   `add_bombs(−2)` only for the 2-bomb case.
Q. `eclvm.rs` `spawn_child` (07 op 93/94, §10.5): ops 93/94 return at once while life ≤ 0; ops 90–92 test life < 1 /
   chain-killed before any argument read; ops 90–92 play SE 0x24 even when the spawn fails.
R. `bomb.rs` `bomb_yuyuko` (14 §2.15): radius ≥ 500 → `areas_off(k)` too; angle `wrap_add(angle, f8)`.
S. `hud.rs` stage load `StartInfo.replay_route`: `Some(rpy.clear_status())` during playback (10 §4 step 3).
T. `eclvm.rs` var 10100 + `enemy.rs` (07 §6 row 10100): a persistent g_Gui+0x108 Timer, set to the timeout at
   `Spell_Start`, `dec(game speed)` every frame (also without a card; held where `Spell_Calc` returns early); var 10100
   reads its `cur`. The same remaining timer should feed the capture time reward / HUD instead of `duration − frames`
   (check 07 §11.1).
U. `enemy.rs` `update_body` (14 §3.1, 07 §10.1 step 6): `offer_aimed` only when flag A bit 6, after `player_collision`.
V. `enemy.rs` manager clock (11 §3.2): a Timer ticked at game speed (zero-filled start, prev = cur = 0);
   `time_rank`: `cur != prev && cur % (2400 − 240·lives) == 0`; the human-rate `> 15` test reads it too.
W. `game.rs` / `effect.rs` (13 §3): link queued screen effects before a calc-chain restart (`on_restart`), not only
   after the whole `task::run`.
X. `eclvm.rs` native 2 (07 §12): no direction tests; angle when any branch ran.
Y. `enemy.rs` `update` / `update_body` (07 §5): pass the live `anm.speed` to `update_tail` and `move_enemy` (after
   `ecl_run`), not the value sampled at the start of the update. Check the other per-enemy timers the same way.
Z. `player.rs` `update_invuln` (09 §6.4): `state_timer.set(0)` when state 3 ends.
AA. `effect.rs` `ScreenEffect::frozen` (13 §3): read GameFlags bit 10 (IMPL120's flag) / freeze byte live, not the
   copy refreshed at the end of `Player_Calc`.
AB. `bullet.rs` `pattern_angle` (07 §9.3): mode 0/1 `(k + aim) + a1`; mode 4 `((i·τ/c1 + π/c1) + aim) + a1`.
