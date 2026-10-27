# RE-Q56 — batch500 top groups on master (Extra rng, stage 1 bullets, 6A/6B gauge); result states 0x11/0x12/0x16

Code: decompile/disassembly of th08.exe (functions per row). Traces (ref500 recordings, original re-runs with the
probe `TH08_SCAN*` / `RNGSITE` lines in re/work/runs/q56/orig_*) only to locate. Scratch port `portcopy` = master
src dfc6e03 + fixes AC–AK + debug dumps (built with CARGO_TARGET_DIR=target/re56; src/ untouched); runs
re/work/runs/q56/fixA*_*.tsv, CHECKPOINT.md.

| # | replay / row | original | spec | port |
|---|---|---|---|---|
| AC | silentselene_52381 r2146 (Ex rng +940) | `BulletMgr_CancelAll` 0x430830 / `CancelBonus` 0x430AA0: bomb-area branch = `ItemMgr_Spawn`(pos, **raw** player+0xE2A90, mode 1); no 9 → two time orbs / −1 → none mapping (that is only the state-1 drop). `Enemy_KillChildren` leaves type-9 cancel circles; a big power item reaching 128 the same frame → CancelAll → type-9 items (mode 1, no RNG). CancelAll calls `Player_InBombArea` twice. orig_52381b RNGSITE 2125–2141 | 08 §4.1 bomb drop, §7 | CODE BUG AC |
| AD | silentselene_80322 r36953 (Ex rng) | `Spell_End` 0x4161B0 tail, every call: boss g_Gui+4 ≠ 0 → flags A bit 27 := 0; +4 := 0; bit 11 := 0. Card 202 (op 155) → card 203 reward by formula (805), not 700; orig_80322 RNGSITE 36940–36960 | 07 §11.1 "Tail of every op 123" | CODE BUG AD |
| AE | silentselene_53174 r2403, gensokyou_th8_udOa03 r2404 (Ex rng ±4) | `ItemMgr_Calc` 0x440500: type handler (power → SetPower → CancelAllToItems …) runs **before** 0x441770 frees the item; the cancel items skip the still-occupied slot. orig_53174c/d item lists vs port dump | 08 §8.4 collection test | CODE BUG AE |
| AF | silentselene_53098 r25949 (Ex) | `Option_Yukari` 0x44E3A0 sub 3: chase (0x44E8D0) whenever pointer ≠ 0, then the exit test → the option moves on the exit frame. orig_53098d O/B lines | 14 §3.1 | CODE BUG AF |
| AG | silentselene_50763 r97520 (6A gauge); 6B group; Ex Ge81, 53473 | `Player_Move` 0x44BE40–0x44BEF4: B > 0 → `Timer_Decrement`(B) 0x418110 (game speed); n from `Timer_GetF`(C) (> 300.0 → 21, else ftol(c/15)). orig_50763 SCANGAUGE | 09 §9.3 | CODE BUG AG |
| AH | silentselene_84084 (s1 bullets) | `Vec3_DivScalar` 0x40C7D0: states 2–4 pos += vel × f32(1/d) | 08 §4.2 | CODE BUG AH |
| AI | silentselene_52723 r8002, 51969, 53714 (s1 bullets) | `EclInterp_Fn7` 0x421180 disasm: h0 = ((t−1)(t−1))(2t+1), h1 = (t·t)(3−2t), h2 = ((1−t)(1−t))t, h3 = ((t−1)t)t, ((h0a + h1b) + h2c) + h3d; ecldata1 sub36 familiars | 07 §5.6 | CODE BUG AI |
| AJ | silentselene_78212 (s1, ecldata1 sub34) | `BulletMgr_PlaceBullet` 0x42F775–0x42F807 (accumulator 0.0 at 0x42F675): mode 2 (j·a2 + a1) + ((i·2π)/c1 + (0 + aim)); mode 4 ((i·2π)/c1 + (π/c1 + (0 + aim))) + a1. Corrects RE-Q54 rows 98/100 | 07 §9.3 | CODE BUG AJ |
| AK | silentselene_82314 r10013 (s1, slot 1170 off-count 2 vs 0) | `CancelAll` / `CancelRadius` memset 0x42E dwords (whole slot, off-screen counter +0xDBA too) vs `Bullet_Free` 0x432170 (state + 2 timers) | 08 §7 "Slot zeroed" | CODE BUG AK |
| AL | silentselene_51844 r104340, 51455 r120926; 6A rng group (14) and 75833, 76075, 51145, 79901 (port rng_ctr −0x3C late in 6A) | `Ecl_Run` case 0x7F (op 127, slot < 0) calls `Enemy_ReleaseOrbitEffects` 0x42A820: the boss's op-128 orbit effects fade and free. Port keeps them → ring cursor/occupancy differ from 6A frame 20912 on (orig_51844f TH08_SCANEFF vs port ring dump, effcmp.py) → at the last card end the ring is full in the port and a death-spark spawn (4 × 4 draws) and one ECL effect find no slot (INFERRED from the RNG sites of r104340; the scratch fix removes the divergence) | 07 §7 row 127, 13 §2.6b | CODE BUG AL |
| Q4 | IMPL129 Q1 (Result_Calc 0x11/0x12/0x16) | `Result_Calc` 0x4584B0 switch on the entry state: 0x11 / 0x16 → scene request 1 + return 0, 0x12 → return 0, **no frame test**. Unreachable: `Result_Register` 0x4582A0 sets 9 / 0x11 / 0x16 (mode 1) or 0x12 (mode 2 = `GameMgr_LoadScoreData` 0x43BBE1, OnRegister called directly, no calc/draw node); `Result_OnRegister` 0x45964D (called by `FuncChain_AddCalc` before linking) turns 0x11 (after the practice hiscore) and 0x16 into 10 by a direct write. Byte scan: only stores at 0x458348/54/66. Side finding: state 10 → 11 is a direct write, frame kept (0x45696F) | 16 §2.1 "Initial state", §2.4 Exit, §2.6 state 10 | CODE BUG Q4a (exits), Q4b (state 10 → 11 frame) |

Effect (scratch AC–AL): of the 208 sync-ok replays that differ on master outside the stage-5 flags-0x800 group,
160 identical with AC–AK (fixAK_all.tsv), 180 with AL (fixAL_left.tsv, fixAL_ok.tsv: no regressions). Ex group 53/53, stage-1 group 25/28, 6A gauge 20/22, 6B gauge 9/9.
With AL: stage-1 group 27/28, 6A gauge 22/22, 6A rng group 14/14.
Left (28, fixAL_left.tsv): 54656 r72127 (stage-1 group member, later stage) and 27 others not analysed.
