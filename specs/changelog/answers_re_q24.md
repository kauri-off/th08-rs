# RE-Q24 — answers to IMPL22 / IMPL23 / IMPL24 / IMPL25 questions (changelog)

Labels: **SPEC FIXED** (spec was wrong or silent, now corrected), **CODE BUG** (spec right, port differs),
**NEW FACT** (new behaviour added to a numbered spec). Run data: `re/work/runs/q24/` (demo2 rngsite, golden / fullrun
port session dumps, fullrun_orig + fullrun_orig2 original scans with the new `B` player-bullet lines of
`re/work/hook/session.inc`; previous probe kept as `*_v8_pre_q24`).

## IMPL24

| Q | label | answer | spec § | evidence |
|---|---|---|---|---|
| 1 | CODE BUG | the cancel sets +0x564 := +0x568, as the spec says | — | code |
| 2 | — | already specified | 07 §10.3a | — |
| 3 | SPEC FIXED | "enemy destroyed" SE: EnemyMgr_Calc uses slot parity; Enemy_KillChildren uses 2 + (k & 1) of the child index (reward ≠ 0 only) | 13 §1.4 | 0x42C660, 0x42ADB0 |
| 4 | CODE BUG | spec right | — | code |
| 5 | NEW FACT | thread-context interrupts: the pending test runs at every context's LOOP; the thread's ip skips, main is pushed onto the thread's stack, main gets the sub, depth += 1; unreachable in shipped data | 07 §5.5 | 0x41C88A; op 125/89 0 uses |
| 6 | CODE BUG | the chain link is written after the spawn-time Ecl_Run (var 10096 = 0 there); tail looked up before | 07 §10.5 | spawn code |
| 7–9 | CODE BUG | spec right (native 31, op 181, death modes 0/3 now detailed) | 07 §12, §7, §10.3 | code |
| 10 | — | spec right; port's alive filter is equivalent | — | — |
| uncited prev/final pos | NEW FACT | spawn writes only pos; prev pos/displacement/offset/final pos come from the zeroed template; first move gives displacement = spawn pos (vars 10085–87; harmless, reads at t 60) | 07 §4.1 | 0x42A4E0, 0x42DEB0, 0x429E00 |
| uncited KillAll self | SPEC FIXED | no self test in KillAll (op 95/123) nor in the threshold/timeout non-boss kill; only the boss flag / flags B bit 6 spare the caller | 07 §7 op 95, §10.2 | 0x42EFB0, 0x42B490 |
| uncited bullet draw | NEW FACT | VM by state 2–5, pos + origin, z 0.05, RGB white keeping alpha, rotation π/2 + angle if tag | 08 §1 | 0x432F20 |
| uncited trail draw | NEW FACT | full after-image draw (sprite copies / strip mode 8, scale/alpha fades, angle mid quirk) | 07 §10.6 | 0x42E140 |

## IMPL25

| Q | label | answer | spec § | evidence |
|---|---|---|---|---|
| 1 | — | spec order verified; no RNG, harmless | 06 §2 | 0x409CE0 |
| 2 | CODE BUG | Stage_Calc early return on bit 10 | 06 §5, 11 §2.1 | code |
| 3 | SPEC FIXED + CODE BUG | sparkle v = dist ≤ near ? dist : fog factor (always true); 0 ≤ z ≤ 1 test missing in port | 06 §11.1 | 0x40A1B0 |
| 4 | — | port right (flag set at end of Stage_Calc) | 06 §5 | — |
| 5 | CODE BUG | spell practice replaces the frame > 699 test | 06 §5 | — |
| 6 | CODE BUG | freeze byte: auras get 0x80000, return | 09 §1 | 0x44C390 |
| 7 | CODE BUG | +0xE2A9C reset only for teams | 09 §3 | — |
| 8 | CODE BUG | game over leaves lives/bombs | 09 §6.2 | 0x44CBF0 |
| 9 | CODE BUG | respawn timer 60 every call; spell practice keeps 30 | 09 §6.3 | 0x44D180 |
| 10 | CODE BUG | per 14 §3.1 | 14 §3.1 | RE-Q21 |
| 11 | CODE BUG | both laser fns test bit 13 | 14 §4 | — |
| 12 | SPEC FIXED | update 3 only in state 1 | 09 §2.1 | 0x450580 |
| 13 | CODE BUG | exact hit-entry order; no extra pos/0x2000 | 09 §4.5 | 0x451670 |
| 14 | — | harmless order (no RNG) | 14 §2.2 | — |
| 15 | CODE BUG + SPEC FIXED | clear frame: loading := 1, score_display; return-3 timing | 11 §4 | 0x439BC7 |
| 16 | — | harmless | 11 §3.1 | — |
| 17 | NEW FACT | Music_Play = load (cmd 1); Music_PlaySlot = play (cmd 2, cfg+0x38 bit 13 → cmd 4 first); StageStart loads 0 and 1/2, plays 0 deferred | 02 §4, 06 hdr, 11 §3.1 | 0x447D56, 0x447E47 |
| 18 | CODE BUG | bit 2 recomputed every GameMgr_Calc | 11 §2.1 | 0x439Dxx |
| 19 | SPEC FIXED + CODE BUG | shake 1/7/2 freeze/removal rules | 13 §3 | 0x45BDC0, 0x45BF10, 0x45B760 |
| 20 | — | start scale always (1,1) (script 104); port equivalent | 13 §2.6 | 0x40BF00 |
| 21 | SPEC FIXED + CODE BUG | list C = DrawBillboard via Camera_Set2D on the playfield viewport (size depends on stage right vector, z ≈ 0.9994); layer 3 never set | 13 §2.4 | 0x427F00, 0x409640 |
| uncited stdfile | — | binary does not filter; only types 0/1 exist | 06 §2 | data |
| uncited START | NEW FACT | (192, 384, 0.49) | 09 §6.4 | 0x44D650 |
| uncited real_death drop | NEW FACT + CODE BUG | ItemMgr_DropAttracted: state 0, vel (0, −0.9, 0) at Player_Die and real death | 09 §6.1 | 0x441530 |
| uncited dying_update | NEW FACT | vx/vy := 0 each shrink frame, +0xE2A70 := 60 every respawn call | 09 §6.2–6.3 | 0x44CBF0, 0x44D180 |
| uncited cooldown | NEW FACT + CODE BUG | +0xE2A6C := 16 only by Spell_End (abs 0x18B8964); decremented at the top of Player_UpdateBomb | 09 §1.1, 07 §11.1 | 0x4161B0, byte search |
| uncited box_in_cancel_area | — | no original counterpart (only the point test 0x449FF0); dead port code | 09 §8 | callers |
| uncited origin_pos | SPEC FIXED + CODE BUG | option pos read without a free test | 09 §4.3 | 0x44FB70 |
| uncited filter 2 | NEW FACT | shared u8 counter +0xE2A94, every 8th: type 5 at (bullet x, enemy y, z) | 09 §2.1 | 0x450EE0 |
| uncited Area::contains | SPEC FIXED + CODE BUG | damage circle = point test on enemy centre; rect AABB / rotated forms | 09 §4.5 | 0x451670 |
| uncited no bomb in state 1 | NEW FACT | counter 0 throughout state 1 | 09 §6.3 | 0x44D180 |
| uncited +0xE2A7C | NEW FACT | := 0 by every failing non-bombing start test; readers items / time orbs | 09 §1.1 | 0x44C650 |
| uncited prio 0x15 | NEW FACT | every ScreenEffect_Register call uses 0x15 except loading darken (1) | 13 §3 | all call sites |
| uncited 22–25 / ring init | NEW FACT + CODE BUG | no init for 22–25 (spawn pos = player pos); ring init default 24 segments (+ overrides) | 13 §2.6 | table 0x4C6D30, 0x4272E0 |
| uncited +0x357 | NEW FACT | only type 59 (Player_Die) | 13 §2 | writers |
| uncited RANK_STATE | NEW FACT | boot 0, 0, 0, 0 (GameMgr_Ctor) | 11 §2 | 0x43C5E1 |
| uncited 0x164D30C | NEW FACT | += 1 at the top of EnemyMgr_Calc outside dialogue | 11 §2 | 0x42C6A0 |

## IMPL22

| Q | label | answer | spec § | evidence |
|---|---|---|---|---|
| 1 | SPEC FIXED | op 36 resolves a–d once at op time (bit copy); later re-read is identity → port right | 07 §5.6 | 0x4213F0, 0x421120 |
| 2 | SPEC FIXED + CODE BUG | time reward is paid as time orbs from the capture ring (min(r,7) + 6 spawns/frame from t > 8; 2 RNG each); the 130-frame payout only adds the score; max point += 0 normally | 07 §11.1 | 0x416B90; golden payout frame has no time jump |
| 3 | SPEC FIXED | no self skip: a non-boss switcher also dies | 07 §10.2 | 0x42B490 |
| 4 | SPEC FIXED (clarified) | cancel + spell flags on every timeout without bit 27, card or not → port right | 07 §10.2 | 0x42B930, 0x42BC50 |
| 5 | — | confirmed: max life unchanged | 07 §10.2 | 0x42B490 |
| 6 | CODE BUG (confirmed) | as IMPL24 Q6 | 07 §10.5 | — |
| 7 | SPEC FIXED + CODE BUG | not the homing bullet: the stage-1 boss's op-67 move at row 6251 goes up in the original (two sequential y tests; band 80 < y < 96 flips twice) and down in the port (`eclvm.rs:594` single `||`) | 07 §8.3 | 0x422020; original scan rows 6250/6251 y 86.71 → 84.70 (port 88.72) |
| 8 golden | CODE BUG (= Q2) | row 8056 = first capture-ring orb frame: 13 spawns × 4 Next16 = the missing 52 draws | 07 §11.1 | port session RNG at 8056 |
| 8 demo 2 | SPEC FIXED + CODE BUG | shrink timer +0xE2AF4 is not reset at the real death (counts from the hit): hit 2907, real death 2922, state 1 at 2938; port still dying | 09 §6.2 | 0x44CBF0, 0x44D2C0; rngsite demo 2 |
| 9 VM 141 | — | 16→17: range 26 → catch-all (640,320) hidden; 17→16: range 18 slides in (already in 16 §6.2) | 16 §6 | 0x46BBC0, script 141 |
| 9 shot change | NEW FACT | per-VM writes only (VMs 111–130), no range interrupt; input from frame counter 8 | 16 §6.1 | 0x46B174 |
| 9 description fade | NEW FACT | interrupt 2 to the description VM on every exit except Option and Quit | 12 §2.2 | 0x4674E0 |

## IMPL23

| Q | label | answer | spec § | evidence |
|---|---|---|---|---|
| 1 | SPEC FIXED + CODE BUG | default stage byte is 0; the ranking maps the byte through 0x4C7F80 = (1,2,3,4,4,5,6,6,1) → "(1)"; defaults have magic "DMYS", shot byte not written | 15 §2.3, 16 §2.3 | 0x45964D, 0x4586B4 |
| 2 | SPEC FIXED | save skips non-HSCR nodes (DMYS) and writes +9 := 0 (and shot/diff/sizes/version) into the in-memory entry before copying | 15 §2.3 | 0x453D0D |
| 3 | NEW FACT | frame 0: interrupt difficulty + 3 to all 72 VMs, nothing else; port right | 16 §2.2 | 0x45621E |
| 4 | NEW FACT | Result_EndBanner writes VM 71 interrupt 18 every state-0xF frame ≤ 30; from frame 90 Shot/Enter → VM 71 only interrupt 2 | 16 §2.1 | 0x4577E2 |
| 5 | NEW FACT | 0x164D30C += 1 per EnemyMgr_Calc outside dialogue (not paused/loading/game-over/post-clear; yes during bit-10 freeze) + 1800 per op 148 | 11 §2 | 0x42C6A0 |
| 6 | NEW FACT | +0x78 = 0x164CFAC += 1 at a last-spell start; +0x74 = 0x164CFA8 at a normal bomb | 15 §2.3 | 0x44C650 |
| 7 | NEW FACT | timer = enemy-manager clock EnemyMgr+0x9DCED0 | 15 §2.3 | 0x42C6A0 |
| 8 | SPEC FIXED | Result_Draw adds pos_offset to every result00 VM (saved/restored around Draw2DSnap) | 16 §2.4, 05 §2 | 0x4586B4 |
| 9 | NEW FACT | text VMs: AnmVm_Init + SetSprite (no script), pos 0, flags 0x1800, font 15 | 16 §2.4 | 0x45964D |
| 10 | SPEC FIXED | SE 12 confirmed | 16 §2.4 | 0x45A1F3 |
| 11 | SPEC FIXED | input accepted on sub-frame N (same call) → port right | 16 §2.4 | 0x454CB2, 0x45567D |
| 12 | — | nothing to answer | — | — |

## CODE BUG list (port to fix)

1. IMPL24 Q1, Q4, Q6 (= IMPL22 Q6), Q7, Q8, Q9.
2. IMPL25 Q2, Q3 (missing 0 ≤ z ≤ 1), Q5, Q6, Q7, Q8, Q9, Q10, Q11, Q13, Q15, Q18, Q19, Q21 (list C billboard).
3. IMPL25 uncited: DropAttracted vel −0.9 state 0 (09 §6.1); bomb cooldown := 16 at Spell_End (07 §11.1); option
   origin without free test (09 §4.3); damage-area circle = point test (09 §4.5); ring default 24 segments (13 §2.6);
   boot rank state 0 (11 §2, harmless); remove `box_in_cancel_area` (no original).
4. IMPL22 Q2 / Q8 golden: capture time reward as time orbs from the ring (07 §11.1) — **sync**.
5. IMPL22 Q7: op 67/178 y-edge tests as two sequential ifs (`eclvm.rs:594`) — **sync**.
6. IMPL22 Q8 demo 2: do not reset the state timer at the real death (09 §6.2) — **sync**.
7. IMPL22 Q3: threshold kill has no self skip (07 §10.2).
8. IMPL23 Q1: default ranking stage byte 0 + stage map 0x4C7F80; IMPL23 Q2: save clears +9 in memory.
9. IMPL24 uncited: spawn prev pos (0,0,0) (harmless), KillAll no self test, trail draw (07 §10.6).
10. Optional/harmless: IMPL25 Q1, Q14, Q16.
