# RE-Q57 — batch500 leftovers after AC–AL (homing speed-up, Alice while bombing, Sakuya follow write, replay overrun); IMPL130 Q1

Code: decompile/disassembly of th08.exe (functions per row). Traces (ref500 recordings, original re-runs with the
probe `TH08_SCAN*` lines incl. the new `TH08_SCANORB`, in re/work/runs/q57/orig_*) only to locate. Scratch port
`portcopy` = q56 portcopy (master dfc6e03 + AC–AL) + AM–AP + debug dumps (CARGO_TARGET_DIR=target/re57; src/
untouched); runs re/work/runs/q57/fix*.tsv, CHECKPOINT.md. Compare tools q57/bcmp.py (player bullets), icmp.py
(items), eb2.py (enemy bullets).

| # | replay / row | original | spec | port |
|---|---|---|---|---|
| AM | silentselene_54749 r4943 (st1 rng), 53470 r5931, udRo70 r12567, 52719 r13150, udBe47 r14072, udRk01 r37491, 52916 r42300, 54427 r47924, udKa49 r55989 (moves on) | `FUN_00450320` (player bullet update 1, homing; disasm 0x450320–0x450563): steering **and** speed-up only while bullet state == 1; state 2 (hit animation) only sets angle = atan2. Speed-up: speed += 1/3, then v = v·speed / sqrt(vx² + vy²) (current \|v\|). Port sped up state-2 amulets → off screen one frame early → slot layout → a time orb at another bullet (orig_54749b scan vs p54749_pb, bcmp.py) | 09 §4.4 | CODE BUG AM |
| AN | silentselene_52858 r45991, 52868 r51932, 52951 r72943, 51361 r37888, udSN28 r51146 | `Player_Move` focus branch zeroes the option slot (0xBD dwords) then state 1; `Option_Alice` 0x44EA40 state 1 → script 0x1D, state 2, falls through to `if player+0xFDC == 0: pos = p − (0, 32)`. Alice created while bombing stays at (0,0,0) until the bomb ends, her laser (update 5) follows (52858 gm 7316: respawn + focus with the bomb flag; orig_52858 O/B lines vs p52858_pb) | 14 §3 row 0x44EA40, 09 §5 table | CODE BUG AN |
| AO | silentselene_54691 r53865, 54601 r52658, 53449 r55933 | `Bomb_Sakuya` 0x40FCD0 / `Bomb_SakuyaLS` 0x4103F0: 0 ≤ T ≤ 60 write p into +0x2A4..+0x2AC of the effect pointer saved at T = 0 (effect 20), no alive/type check. Effect 20 ends at age 39; its slot is reused by a type-51 particle, whose update recomputes +0x2A4 from world +0x2D4 → harmless in the original. The port's single effect pos (= world for 19/51/63) is clobbered → the particle is culled (dot < 0.94) a frame later → ring layout → +10 ANM draws (54691 gm 4158; orig_54691e `TH08_SCANEFFSLOT` 156 vs port Q57EFFSLOT) | 14 §2.7, §2.8; 13 §2.6a | CODE BUG AO |
| AP | silentselene_54320 r75962, 83293 r80041, 51626 r93252 (stage 5, input ≠ 0) | `Replay_PlayCalc` 0x452550: g_Input = *(u16*)(+0x50), +0x50 += 2, no bound check; `Replay_PlayStageLoad` sets +0x50 = record + 0x24 inside the decompressed image. When playback runs more frames than recorded (stage-5 post-boss dialogue with 3× restarts) the reads continue into the following image words (54320: 0x0105 0x05BD 0x0470 = stage 6 record +0 / +4). Port fed 0 | 15 §3.3 | CODE BUG AP |
| Q1 | IMPL130 Q1 (type −1 to `ItemMgr_Spawn`) | `ItemMgr_Spawn` 0x4400A0: no type check. Type byte 0xFF, mode as given, VM script = int type + 0x3D (`add edx, 0x3d` at 0x4403BC) = 0x3C (etama.anm script 0x3C: sprite 193, 10-frame flash, hide_stop). `ItemMgr_Calc` switch has no case for 0xFF → collection does nothing except stats bit 0x40, sound 0x15, free. `ItemMgr_Draw` sign-extends the type (`movsx` 0x441644 / 0x4416F4) | 08 §8.2, §8.3, §7 | CODE BUG Q1 (`ItemMgr` spawn guard for type < 0) |
| — | side finding while reading `ItemMgr_Calc` | case 3 (bomb item): `GameMgr_RankAdd(5)` after the bombs test, always | 08 §8.5 (spec only; port already right) | port right |
| — | side finding (udKa49 r120959 lead) | `GameMgr_AddScore` 0x4181F0: score += n/10, no cap; the cap is GM calc step 9 / unregister | 11 §5 functions list | CODE BUG (likely) `Stats::add_score` caps; not verified by a run |

Effect (scratch AM–AP together, fixAll.tsv): of the 28 leftovers 19 identical (AM 8, AN 5, AO 3, AP 3); the 160
sync-ok replays of the q56 ok set stay identical (okAll.tsv, 160/160).

Left (9, fixAll.tsv; stage index 0 = stage 1, first differing fields):
- st3 (1): 51477 r28764 score +1.
- st4A (3): 54656 r72127 bullets −1 (4A familiars, ECL orbit ulp before frame 22127; RE-Q57 lead, not resolved),
  udPn85 r68957 bullets +1, 51759 r68154 rank +1.
- st4B (4): udCs13 r57766 rng, 52339 r64823 bullets +1, 54010 r56748 score +1, 79317 r62108 bullets −1.
- 6B (1): udKa49 r120959 score 0x3CBD0280 / 0x3B9AC9FF (port capped at 999 999 999; see the AddScore row).
