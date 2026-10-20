# RE-Q27 — next sync divergences, IMPL29 / IMPL30 questions (changelog)

Labels: **SPEC FIXED** (spec was wrong or silent, now corrected), **CODE BUG** (spec right, port differs),
**NEW FACT** (new behaviour added to a numbered spec). Run data: `re/work/runs/q27/` (not in git):
- `golden_orig/`, `fr_orig/`: partial replays of golden / fullrun_noclip in the original with `TH08_SCAN` (items);
  `golden_port/`, `fr_port/`, `st_port/`: the port's `session` dumps (`TH08_SESSION_DUMP`); `itemcmp.py` diffs them.
- `demo_orig1/`, `demo_orig2/`, `probe_opts` (demo 3 only): attract-demo probe runs (`iscan` now with `A` bomb-area and
  `P` player-bullet lines, `oscan` = option blocks, `rngsite`, `escan`, `bscan`); `demo_port/`: port `demosync` dumps.
- `actr_orig/actr.txt` (original, `TH08_ACTR=1`) vs `actr_port.txt`: ASCII counter +0x8284 per golden row.
- `portfix/` + `portfix.diff`: a scratch copy of the port with the CODE BUG fixes below applied, used only to confirm
  each root cause (src/ untouched). Results in the last column of §1.

## 1. First divergences

| session / row | root cause | label | spec § | after the fix (scratch port) |
|---|---|---|---|---|
| golden 8105, fullrun_noclip 7553 | not orb collection: the capture-ring orbs spawn at a different centre. During the card the original pulls the start ring's centre toward boss pos + offset by 1/16 per frame (`Spell_Calc` card-active branch); the port leaves it at the spawn point. At capture the centres differ by ~10 px (golden) / ~4 px (fullrun); the offset decays by 15/16 per frame afterwards (orb positions orig vs port differ by a constant per batch, ratio 0.9375 between batches), so orbs reach the player a frame apart | SPEC FIXED + CODE BUG | 07 §11.1 (Spell_Start ring, Spell_Calc steps 1–3, capture ring), 13 §2.7, 12 §5.8 | golden 8177, fullrun 7625 |
| golden 8177, fullrun 7625 (exposed by the fix) | paid card bonus: the bonus decays **every frame** by trunc((R/60)·gs) (R = per-second rate +0x100) and is rounded down to 10, only while capture is possible; `Spell_AddBonus` raises R by n/120 and caps at +0x2638. The port decays R once per second and has no cap / R increase | SPEC FIXED + CODE BUG | 07 §11, §11.1 step 1.3 | golden 8298, fullrun 7746 (rng −8, bullets 2/1; not chased) |
| demo 0 4451 | bullet 182 (sticky +0xDBE mark from areas 192–198 at 4444–4445) drops at its effect end with player +0xE2A90 = 7 left by bullet 181's hits; the port's `in_bomb_area` resets the item to 6 on every call, so it drops a star (type 6, no RNG). +0xE2A90 is reset to 6 only by the hit/graze tests | SPEC FIXED + CODE BUG | 08 §4.1 (bomb drop), 09 §8 | 4629 |
| demo 1 3870 | `GameMgr_AddTime` adds n to the parity accumulator +0x44 also for negative n; the death-bomb window 3844–3854 (11 × −15) flips the parity in the original, not in the port → max point +10 off → point item value | SPEC FIXED + CODE BUG | 11 §1.5 setters, stats table +0x44 | 3880 (gauge; not chased) |
| demo 2 3082 | same as demo 1 (death at 2907–2922: drain and death penalty) | CODE BUG (same) | 11 §1.5 | 4036 (rng; not chased) |
| demo 3 1606 | Remilia team options (shot 2): the orbit turns only when the option timer is **> 12** (`Timer_Greater`), spec said ≥ 12; the port rotates one frame early, so the option-origin shots spawn ~0.2 px off and one reaches the boss hitbox a frame early (port hit 1606, original 1607) | SPEC FIXED + CODE BUG | 14 §3 table (0x44EB70), §3.2 | 6665 |
| selftest1 1423 | ESC frame: GM flags bit 2 is recomputed in step 7 of `GameMgr_Calc`, after the pause test, so it is already 0 on the pause frame; the port never recomputes it | CODE BUG (11 already said so; step placement added) | 11 §2.1 flags row 2, §4 step 7 | 1540 (frame counter 0 after the pause-quit, 11 §2 — spec already right) |

## 2. IMPL29

| Q | label | answer | spec § | evidence |
|---|---|---|---|---|
| 1 ring timer at `Spell_End` | SPEC FIXED (port right) | yes: `Spell_End` sets the ring's age timer +0x338 := 0 (and restyles the ring) before moving it to +0xF8 | 07 §11.1 capture ring | 0x4161B0 |
| 2 step order | SPEC FIXED (port right) | the card-active step / ring step runs **before** the 13 VMs, then the remaining-time decrement. It runs whenever the card is inactive and +0xF8 ≠ 0, including after native 22 (which clears bit 0); only GameFlags bit 10 / the freeze byte stop it | 07 §11.1, 12 §5.8 | 0x416B90 |
| 3 orb angle | SPEC FIXED (port right) | ((t − 10)·6.2831855)/40 − 1.5707964, left to right, f32 rounding per op (main thread CW 0x007F), t = GetF; then `Angle_AddNormalize`(a, 0) | 07 §11.1 | 0x416B90 asm, 11 §1.2 |
| 4 t 30 / 60 restyle | NEW FACT | exact VM slot writes (pos slot dur 100 mode 4 → (0, 60); scale slot dur 20 → 64, then dur 70 → 0); +0x32C := VM pos.y every frame | 07 §11.1 | 0x416B90 |
| 5 ring rotation | SPEC FIXED + CODE BUG | `Angle_AddNormalize` in both branches (inactive −0.05235988 / −0.015707964; active +0.015707964, last spell −0.031415928) | 07 §11.1 | 0x416B90 |
| 6 respawn clear order | SPEC FIXED (port right) | the +0xE2A70 decrement + cancel rect is the first step of `Player_UpdateInvuln` | 09 §6.4 | 0x44D2C0 |
| golden 8105 / fullrun 7553 | see §1 | | | |

## 3. IMPL30

| Q | label | answer | spec § | evidence |
|---|---|---|---|---|
| 1 ASCII counter +0x8284 | SPEC FIXED + CODE BUG | only writer: += 1 at the end of every `Ascii_Calc`; never reset; the ASCII manager is registered by `Supervisor_Init` on the boot loader thread, so the 13 boot calc passes of a session do not count: counter == main-menu frame number. The port counts its boot calcs (+13 ≡ +1 mod 4 → blink one frame early) | 12 §5.10 | dispscan 0x8284; `actr_orig` vs `actr_port` (constant diff 13 from row 0 to 4400) |
| 2 `Enemy_Free` of a boss | SPEC FIXED (port right) | tests flags A bit 1 **and** slot byte < 4; op 127 −1 clears bit 1, not the byte; byte written only by op 127, template 0 | 12 §5.10 | 0x42BCF0, 0x41C562 |
| 3 mode-0 boss marker | SPEC FIXED + CODE BUG | mode 0 keeps the boss table entry and bit 1; the death frame still updates the marker, later frames skip the dead slot → marker stays visible at its last position / mode until op 127 −1, a new op 127 on the slot, or the next stage. The port's `release()` unregisters | 12 §5.10, 07 §10.3 row 0–1 | 0x42C6A0 |
| 4 hit-flash sfx | SPEC FIXED (port right) | the flash flag is set when the **raw** damage > 0, before all reductions; sfx plays even when the reduced damage is 0 | 07 §10.1 step 7 | 0x42D367, 0x42DB62 |

## 4. Other findings (numbered specs)

| fact | label | spec § |
|---|---|---|
| start ring is a wavy ring (6 waves, amplitude 64 → 0 over 100 frames, then plain), shrinks 256 → 8 over the rest of the card, failed look (sprite 0xDE, scale 4) once capture is lost | SPEC FIXED (was "pos.y unused") | 13 §2.7, 07 §11.1 |
| card ends silently (bit 0 := 0, `Spell_HideName`) when the boss is dead or its slot changed | NEW FACT | 07 §11.1 step 1.1 |
| bonus-line tint alpha +0x2623 fades 0x80 ↔ 0x20 by 4/frame with the player under the boss name (x ≥ 64, y < 64) | NEW FACT | 12 §5.8, 07 §11.1 step 1.2 |
| `Spell_Start` spawns the ring at boss pos e+0x2D34 (port uses final pos; equal when the offset is 0) | NEW FACT | 07 §11.1 |
| `Player_InBombArea` is a pure point test; the second argument (bullet size) is ignored | (confirmed) | 09 §8 |

## 5. Tools

- Probe (`re/probe/hook.c`, `session.inc`; previous build `re/work/hook/d3d8_v9_pre_q27.dll`): `iscan` also writes `A`
  (bomb areas, raw 16 dwords, index 0–191 damage / 192–383 cancel) and `P` (player bullets) lines; new `oscan K F0 F1` →
  `probe_opts.txt` (player + 4 raw option blocks +0x2A4..+0x2F0); session mode `TH08_ACTR=1` → `actr.txt` (poll, ASCII
  counter, scene) per trace row. `p1 poke1 164d0bc 2` makes the first attract demo demo 3 (one-demo runs of ~3 min).

## CODE BUG list (priority: sync first)

1. **Capture/start ring follows the boss** during the card: centre += (boss pos + offset − centre)/16 per frame unless
   spell bit 6, z := 0 (07 §11.1 step 1.6) — golden 8105, fullrun 7553. Spawn the ring at e+0x2D34, not final pos.
2. **Spell bonus per frame**: −= trunc((R/60)·gs) (R = +0x100), then −= bonus % 10, only while capture is possible and not bit 11 /
   flag A bit 27; `Spell_AddBonus`: bonus += n, R += n/120 if below max +0x2638 else bonus := max (07 §11, §11.1) —
   golden 8177, fullrun 7625.
3. **`in_bomb_area` must not reset the cancel item**: +0xE2A90 := 6 only at the start of the hit / graze / no-bomb hit
   tests (08 §4.1, 09 §8) — demo 0 4451.
4. **`add_time`**: time_added (+0x44) += n for negative n too (when not clamped) (11 §1.5) — demo 1 3870, demo 2 3082.
5. **Remilia options**: orbit turns when timer **> 12** (team and solo) (14 §3, §3.2) — demo 3 1606.
6. **GM flags bit 2** recomputed every `GameMgr_Calc` at step 7, after the pause test, before the step-8 return
   (11 §4) — selftest1 1423.
7. **ASCII counter +0x8284**: do not count the boot calc passes; it must equal the main-menu frame number (12 §5.10) —
   golden marker blink one frame early.
8. Ring rotation through `Angle_AddNormalize`; active-card rotation (+π/200 per frame, −π/100 for a last spell); ring
   shrink after 100 frames; wavy start ring (+0x32C from VM pos.y while +0x334 ≠ 0); failed-ring look; t 30/60 restyle;
   capture restyle colour 0x20D080A0 / waves 6; card ends silently when the boss is gone (07 §11.1) — draw only.
9. Bonus-line tint alpha fade (12 §5.8) — draw only.
10. Mode-0 boss keeps its marker (no unregister in `release()`) (12 §5.10).

Next divergences after fixes 1–6 (scratch port, not chased): golden 8298 and fullrun_noclip 7746 (rng −8 draws, bullets
2/1 — same pattern in both), demo 0 4629, demo 1 3880 (gauge), demo 2 4036, demo 3 6665, selftest1 1540 (GM frame counter
after a pause-quit, 11 §2 already says it reads 0 → CODE BUG).
