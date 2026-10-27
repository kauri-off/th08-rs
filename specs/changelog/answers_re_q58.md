# RE-Q58 — last 9 batch500 leftovers (AddScore cap, Reimu bomb orb angles, ECL fsin/fcos rounding)

Code: disassembly of th08.exe (addresses per row). Traces (ref500 recordings, original re-runs in
re/work/runs/q58/orig_*, port probe lines QA/QD/QS/QE/QM) only to locate. Scratch port `portcopy` = q57 portcopy
(master dfc6e03 + AC–AP) + the fixes below + debug dumps (CARGO_TARGET_DIR=target/re58; src/ untouched); runs
re/work/runs/q58/{fixQ1,fixAQ,fixAR,fixAS,final,okAQ,okAR,okAS}.tsv, CHECKPOINT.md.

| # | replay / row | original | spec | port |
|---|---|---|---|---|
| Q1 | gensokyou_th8_udKa49 r120959 (6B score 0x3CBD0280 / 0x3B9AC9FF) | `GameMgr_AddScore` 0x4181F0: score += n/10, no cap (RE-Q57). Scratch `Stats::add_score` without `.min(999_999_999)` → udKa49 identical | 11 §5 (already written in RE-Q57; confirmed by run) | CODE BUG Q1 |
| AQ | silentselene_51477 r28764 (st3 score +1), 52339 r64823, 54010 r56748, 79317 r62108 (4B) | `Bomb_Reimu` 0x40C010 (call 0x43EDB0 `Angle_AddNormalize` at 0x40C24B) and `Bomb_ReimuLS` (call at 0x40CB4C): orb angle ±π/60 per frame is added **with wrap** into [−π, π]. Port plain `+=` → orb position ulps off → area 15 hits the boss one frame earlier (51477 st3 frame 3719: boss damage 45 vs 40; orig_51477 A lines vs port QA) | 14 §2.1, §2.2 | CODE BUG AQ |
| AR | gensokyou_th8_udCs13 r57766 (4B rng) | `Bomb_Reimu` 0x40C0F0 loop (`fld` a / `fadd` [0x4B439C] / `fstp` a, same in `_LS`): start angle is a running f32 sum a := −π, angle_i = a, a += π/8 (f32 0.3926991) — not −π + i·π/8 (1–5 ulp off for i = 2–5, 7–12, 14). Cs13 4B frame 14887 orb 2 area 1 ulp → +1 hit spark (+4 RNG) at 14922. `Bomb_Yuyuko` 0x413140 really uses i·π/8 − π (port right there) | 14 §2.1 (§2.2 refers to it) | CODE BUG AR |
| AS | silentselene_54656 r72127 (4A bullets −1), gensokyou_th8_udPn85 r68957 (4A bullets +1) | `Ecl_Run` case 0x20 / 0x21 (ops 32 / 33): `Math_Sin` / `Math_Cos` (CRT `fsin`/`fcos`, extended, st0 unrounded) then `fstp` f32 into the dest var = f32 of the extended value, rounded once. Port op 32/33 uses Rust `f32::sin/cos` (libm `sinf`/`cosf`, not correctly rounded): x = 0xC0045DCE `cosf` 0xBEF44F54 vs f32(cos x) 0xBEF44F55. ecldata4a sub 43 ($10078 = cos·(−π/40) = familiar orbit angular speed) frame 21579 1 ulp (orig_54656h M lines vs port QM) → orbit → bullets. Scratch: `(b as f64).sin() as f32` | 07 op 32/33; 05 §10 trig sites (new row entry + rounding rule) | CODE BUG AS |
| — | side check for AS: `Stage_Calc` sway (disasm 0x40869F–0x40880A) | every sway value = `Math_Sin`/`Math_Cos`, `fmul` f32 constant (40, 70, 0.1, 1.0), `fstp`: f32(sin φ·k) rounded once | 06 §7 | CODE BUG AS-b (port f32(sinf φ)·k) |

## AS — other port sites with single-precision sin/cos (05 §10 trig sites)
The original never computes sin/cos in single precision: every site is either f32(trig·r) rounded once from the
extended product or f32(trig) rounded once, then f32 arithmetic. Port sites using `f32::sin/cos/sin_cos` (master
5facc45), with the model the spec gives:

| port site | original (05 §10 row) | correct model | logic? |
|---|---|---|---|
| `src/eclvm.rs` `exec` op 32/33 | ECL ops 32/33, rounded first | `(b as f64).sin() as f32` | yes (AS) |
| `src/stage.rs` `sway` | `Stage_Calc` sway, call; fmul (06 §7) | `((phi as f64).sin() * k as f64) as f32` per entry; mode 3 up.z `(phi as f64).cos() as f32` | yes (camera facing → effect cull 13 §2.6a) |
| `src/player.rs` `Player::laser_test` (Vec2_Rotate by −a) | `Vec2_Rotate` 0x43EE30, rounded first | sn, cs = f32 of f64 sin/cos | yes |
| `src/bomb.rs` `Area::hits_enemy` and `Area::contains` (`(-self.rot).sin_cos()`) | `Vec2_Rotate` (`Player_InBombArea`), rounded first | same | yes |
| `src/bullet.rs` `cancel_lasers` | `Math_SinCosF32` 0x433880, rounded first | same | yes (item positions) |
| `src/effect.rs` `type_init` 17/18/27 (`EffInit_Converge`) | rounded first | dir = (f32 cos, f32 sin) of f64 | yes (effect positions → cull/RNG) |
| `src/effect.rs` `type_init` burst (`a.cos() * m`) | `EffInit_Burst`: f32(f32(cos)·m) | f32 cos of f64, then f32 ·m | yes |
| `src/effect.rs` `update` orbit 13 (`EffUpd_Orbit13`) | rounded first | same | yes |
| `src/anmvm.rs` `AnmVm::run` ops 61/62 | `AnmVm_Run`, rounded first | f32 of f64 | ANM vars (draw unless read back) |
| `src/bullet.rs` `draw_lasers`, `src/anmmgr.rs` rotated / billboard corners, `src/enemy.rs` ring draw, `src/effect.rs` `EffDraw_Ring`, `src/gfx.rs` `Mat4::rotation_*` | draw (`Math_SinCosF32`, ANM rotations, `EffDraw_Ring` = Vec3_FromPolar model) | same models | draw only (parity policy: visually identical; optional) |

Already right: `bullet::polar` and `bomb.rs` `sin_r`/`cos_r` (f64 product, rounded once). Not checked against a run:
the rows other than op 32/33 (no replay left that needs them); they follow from the 05 §10 table (VERIFIED RE-Q31).

## Effect
All four fixes together (scratch, final.tsv): of the 9 leftovers 8 identical (Q1: udKa49; AQ: 51477, 52339, 54010,
79317; AR: udCs13; AS: 54656, udPn85). The q57 ok set plus the fixed replays stay identical (okAS.tsv, 168/168, same
binary). Left: silentselene_51759 r68154 (4A) rank 0x8 / 0x9 — not analysed (brief stop after 3–4 causes).
