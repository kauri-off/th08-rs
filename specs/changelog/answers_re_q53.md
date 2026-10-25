# RE-Q53 — replay pilot: next earliest full-game divergences

Code: decompile/disassembly of th08.exe (`FUN_00450240` = shot init 1, `Player_InitShot` 0x44FB70, option calc fns
0x44EB70 / 0x44EE70 / 0x44F2D0 / 0x44F5E0 / `Option_Yuyuko` 0x44F930, `Angle_AddNormalize` 0x43EDB0, `Ecl_Run` case
0x60–0x68, `Ecl_FireBullets` 0x422720 (+ disassembly 0x422A6C–0x422AC3), `Enemy_AutoFireAndDirAnim` 0x423150,
`Player_DamageAgainst` 0x451670, `PlayerBullet_HitExplosion` 0x450C50, `FUN_004178a0` card flag).
Traces only to locate. Runs in re/work/runs/q53/: `orig_51842`, `orig_50715`, `orig_80412`, `orig_53084`,
`orig_Bi01` (TH08_SCAN), `orig_53084r` / `orig_Bi01r` (TH08_RNGSITE; probe_rng.txt copied from the slot game dir),
`orig_53084b`–`d` (TH08_SCAN with the new E-line fields `acc` = e+0x2E10 and `step` = 0x18B8A24). Scratch port
`portcopy` (src snapshot of impl119 + q52_scratch.diff H–J, built with target/re53; src/ untouched) → `fixHJ`, `fixK`,
`fixL`, `fixM`, `fixN`; changes in `q53_scratch.diff` (also contains debug dumps TH08_Q53DMG / TH08_Q53RNG, not part
of any fix).

| # | question | original | spec | port |
|---|---|---|---|---|
| 1 | 51842 r2891 `score` (+3, stage 1 frame 2702, Yukari solo) | Shot init 1 (Yukari's aimed shot) aims at the aimed enemy's **position +0x2D34**, not its final position +0x2D88; the angle is `Angle_AddNormalize`(atan2, f32(entry angle + π/2)). The aimed enemy (slot 6) is an op-92 follower, so +0x2D34 is its offset from the parent (−5.9, −3.8): the original's shots fly up-left (−153°…−164°), the port's toward the follower's playfield position (+164°…+174°); one port shot hits enemy 5 (19 dmg → +3 score) | 09 §2.1 init 1 row, 14 §3.1 sub-3 note | CODE BUG K |
| 2 | 50715 r3680 (`rng`/`score` +4, Yuyuko options, frame 3491), 80412 r4011 (−4, Remilia solo, frame 3822) — and 51852 r4717 (Youmu solo, frame 4528), same cause | Every option orbit step is `Angle_AddNormalize`(orbit, step), so the orbit stays in [−π, π] (Remilia team/solo, Youmu half/solo, Yuyuko). The port adds without wrapping (16.755 vs −2.0944 at frame 3466); polar(orbit, 6) is then a few ulps off, the option position (y 0x43A4DC80 vs 0x43A4DC7F) and every shot spawned from it (`Player_InitShot`: option pos + offset; x 0x431F0FB4 vs 0x431F0FB7) drift, and a shot at x = 129.000046 vs 129.0 touches the boss box edge (x 120) one frame early in the port. 80412 / 51852: the same drift with the other sign | 14 §3 "Orbit wrap" paragraph, table rows 0x44EB70 / 0x44F930, §3.2, §3.3 | CODE BUG L |
| 3 | Bi01 r7416 (`rng` +2, stage 6A frame 7227) | `Ecl_FireBullets` tests its filters on the raw instruction **before reading any argument**: mode flags 0x8000/0x10000 against the **enemy's** flag A bit 11 (not the player mode), then the no-fire radius. A skipped instruction draws no RNG. The port decodes the arguments first (a `$rand` angle = 2 draws) and filters afterwards: an enemy 28 px from the player (radius 32) drew in the port only. Same code read: argument order type, count1, count2, angle1, speed1, angle2, speed2, rank step, colour last; speed2's rank add is unconditional (only speed1 is gated by ≠ 0; disassembly 0x422A7A jumps to the speed2 code); auto-fire re-runs the same function on the stored copy | 07 §9.1 (rewritten as steps), §9.2, flags A bit 11 | CODE BUG M |
| 3b | 53084 r7021 (`rng` −4, Extra, Marisa solo, frame 6832: 1 time orb instead of 2) | Time-orb accumulator e+0x2E10 of the boss was 1 lower in the port since frame 6794. The boss died at 6793; at 6794 its own `Ecl_Run` (step 4) ends the card, then its step 6 runs `Player_DamageAgainst`. The explosion hit filter (0x450C50) tests the card flag **live** (`g_Gui` via 0x4178A0) → no card → hits on every 2nd frame, so explosion B42 (timer 62) hits in the original (+1). The port's filter reads `env.spell`, a copy taken at the end of the previous `EnemyMgr_Calc` → still "card" → every 4th frame → no hit | 09 §2.1 hit 1 row | CODE BUG N |

Confirmation (scratch port with H–J + K–N, all 22 pilot sessions, re/work/runs/q53/fixN):
- 0 differing rows unchanged: Ar02, Bu08, F128, JS06, Nd115, Ro168, Tt16.
- Moved: 51842 2891 → 69097 (bullets), 50715 3680 → 8951 (rng), 80412 4011 → 79679 (flags), 51852 4717 → 67035 (flags),
  Bi01 7416 → 21879 (rng), 53084 7021 → 35190 (rng), Dn14 7339 → 8210, 51249 8330 → 20733, 52089 18346 → 49845,
  52104 22171 → 87347, 52220 16139 → 49637, 82309 6870 → 57238.
- Unchanged: 53373 50932, 80406 76752, 88234 62194 (not analysed).
- Per fix: K alone moved 51842 to 17925; L moved 50715 / 80412 / 51852; M moved Bi01, 51842, 80412 further; N moved 53084.

Tooling: re/probe/session.inc scan E lines gained `acc` (e+0x2E10) and `step` (0x18B8A24). TH08_RNGSITE writes
probe_rng.txt into the game dir (re/work/runs/rpy/slotN/game/), not into the run's out dir — copy it after the run.
Scratch-port dumps: `TH08_Q53DMG="f0 f1"` (H hit / D damage+acc / B bullet / O option / F fire lines, port frame =
original frame + 1 at that call site), `TH08_Q53RNG="f0 f1"` (per-draw Rust backtraces keyed by trace frame).

## CODE BUG list
K. `pshot.rs` `init_bullet` init 1 (Yukari aimed shot, 0x450240): aim at the aimed enemy's `pos` (e+0x2D34), not
   `final_pos` (`Player::aimed_pos` is the final pos and stays right for the option chase, 14 §3.1) — e.g. a second
   copied field `aimed_base` refreshed with `aimed_pos` at the end of `EnemyManager` calc; angle =
   `wrap_add(atan2(dy, dx), e.angle + FRAC_PI_2)` (entry angle + π/2 summed first), 09 §2.1.
L. `pshot.rs` option calc (`OptKind::Remilia | RemiliaSolo`, `Yuyuko`, `YoumuHalf | YoumuSolo`): orbit step must be
   `o.angle = anmvm::wrap_add(o.angle, step)`, not `o.angle += step` (14 §3 "Orbit wrap").
M. `eclvm.rs` `bullet_op` / `autofire_and_dir_anim` / `decode_bullet` / `fire_desc` (07 §9.1–§9.2): (a) run the
   mode and no-fire filters on the raw instruction before `decode_bullet` (no variable reads, no RNG when skipped),
   in both the op path and the auto-fire path; (b) the mode filter uses the enemy's flag A bit 11 (`A_YOUKAI`), not
   `g.player_mode`; (c) decode order type, count1, count2, angle1, speed1, angle2, speed2, colour; (d) speed2's
   rank add (`+ lerp_f/2`, min 0.3) also when speed1 == 0.
N. `pshot.rs` `hit_filter` (explosion, 0x450C50) reads `self.env.spell`, which `EnemyManager` writes only at the end
   of its calc: it must see the card state at the moment of the damage call (e.g. refresh `p.env.spell` from
   `self.g.spell` right before `damage_against` in `enemy.rs` step 6, as the scratch did, 09 §2.1 hit 1). Check the
   other `env.spell` readers called from inside `EnemyMgr_Calc` the same way.
