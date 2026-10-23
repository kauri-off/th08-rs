# RE-Q32 — draw parity (golden draw log), IMPL86/89/90 questions

Runs: `re/work/runs/q32/` (scratch port `portfix/` = master 2c3bb84 src + fixes tagged `// q32 X`; `rungolden.sh <tag>`
= golden session with draw log, report `golden_<tag>/index.html`). The logic trace stayed full (25634 rows) for every tag.

## 1. Draw divergences (golden), in the order fixed

| tag | draw group | cause | spec | after (same quads / identical frames) |
|---|---|---|---|---|
| base | — | master | — | 87.91 % / 723 |
| H | front.anm#0 +3.66 M, ascii +432 k | panel repainted every frame; original: dirty-field rule, full-repaint condition, loop bounds, string order | 12 §5.3 (per-frame order, dirty writers, bounds) — SPEC FIXED (writers were missing) | 87.91 % / 723 (extra 4.67 M → 581 k) |
| I | ascii row 40 | hiscore continues stats+0x18 comes from score.dat top entry; rows 56 before 40 | 11 §5 0x18, 15 §2.3 — NEW FACT | 88.09 % / 1675 |
| P | ascii popups pos / z / zwrite | popup VMs are centred (AnmVm_Init undoes the 0x1800 anchor), Draw2DSnap, small z 0.1 | 13 §4 — SPEC FIXED (spec said left-anchored) | 90.67 % / 1823 |
| P2 | ascii popup alpha | fade distance = player playfield pos − popup screen pos (no origin correction) | 13 §4 — SPEC FIXED | 92.01 % / 2614 |
| S | etama#1 +377 k (bg particles) | Stage_DrawHigh draws list-B effects and the spell fade rect only while +0xB24 < 2 | 06 §10.2 — SPEC FIXED | 92.10 % / 2614 |
| J | etama#1 colour (time orbs) | ItemMgr_Draw sets sprite/colour only when the +0x2D6 on-screen byte flips | 08 §8.6 — SPEC FIXED | 94.36 % / 3677 |
| P3 | ascii large popup colour, popup lifetime | port bug (BGRA order); freed at age > 60 / > 90 | 13 §4 — SPEC FIXED (≥ → >) | 96.22 % / 3830 |
| E | enemy.anm colour 25 k, z 63 k | flags A bit 11 → colour2 (0x20,0x20,0xC0,a/2) every frame; enemy VMs drawn at z 0.25/0.3 (port passed 0) | 07 §10.1 step 7 — SPEC FIXED (wording), 07 §10.6 (already right) | 96.49 % / 3904 |
| R | etama#2 2D z 496 k | ring vertices z 0 | 13 §2.5 — NEW FACT | 96.49 % / 3904 (2D z 505 k → 11 k) |
| A | etama#1 aura colour | ops 80/81 bit 1 toggle the aura's colour2 flag; op 174 interrupt + odd-slot spin | 07 ops 80/81/174 — SPEC FIXED | 96.65 % / 4138 |
| L | front.anm +51 k at stage 2 | Hud_Load on a continuing stage keeps front VMs (no VM-13 200-frame repaint, no panel fade-in) | 12 §5.1a — NEW FACT | 96.66 % / 4270 |
| B | `-` (boss bar markers) | op 158 word 1 = start, word 2 = end, fractions of the op enemy's max life; bar quads at exact D3D coords | 07 op 158, 12 §5.1 / §5.4 — SPEC FIXED | 96.85 % / 4270 |
| G | ascii large popups x −3.5·… | large-popup count = stored glyphs incl. '+' (spec said glyphs + 1) | 13 §4 — SPEC FIXED | 97.41 % / 6394 |
| F | stg1bg state 5.9 k | port draw-log writer printed fog start/end with 2 decimals; probe uses %g | FORMAT.md (tool) | — |
| C | capture.anm 26.9 k, loading00 | msg op 9 stage-clear VMs (+0x3778 loadingNN, +0x3CC0 capture, 8 ghosts; capture after the next Present) and the continuing-stage 168 transition tiles not in the port | 10 §9 (already right), 12 §5.1a — NEW FACT (Hud_Load) | 97.82 % / 6806 |
| D | stg2bg / particles during the clear panel | the "HUD condition" (`FUN_00437d87`: +0x398C ≥ 0 and VM +0x3778 waiting for an interrupt) also hides stage layers 0–3 and list-B effects | 06 §10.2 — SPEC FIXED (condition was unnamed) | 97.82 % / 6806 (extra −23 k) |
| K | ascii "+N(M)" popup 6/frame | the chain-count `Popup_LargeStatic` is issued in `Hud_DrawBoss`'s guarded part (port: every calc) | 12 §5.4 (already right) | 97.82 % / 7413 (extra −81 k) |
| T | boss timer digit 1 frame early; dialogue box −0.5 px | HUD +0x24 written by `Enemy_CheckTimeout` before the timer tick; Screen_FillRect3 exact coords | 12 §5.1 +0x24 — NEW FACT | 97.84 % / 7420 |
| U | `-` boss bar width, gauge colour | bar target = life / max life e+0x2E00 (port used e+0x2E04, which the game never reads); remaining-bars bottom colour 0x202020 | 12 §5.1 +0x30, §5.4, 07 §4 0x2E04 row — SPEC FIXED | **98.06 % / 7426** |

Tool fact (FORMAT.md §3): the probe logs at most 4096 vertices per draw call, so reference batches > 682 quads are
truncated (`P list 4`); most remaining ascii "extra" quads in pause frames after mass cancels are this artifact.

## 2. Questions

- **IMPL86 Q1** (op 128 words): word 0 is unused; a, b, c, d = words 1–4 (VERIFIED case 0x80). 07 op 128 row — SPEC FIXED. Port right.
- **IMPL86 Q2** (op 88 return ip): neither — `Ecl_CallSub` gets EDX = the **target context's own stored ip**, so the
  target ctx ip := that ip + size of the instruction there (that instruction is skipped on return, like interrupts);
  the pushed copy holds this advanced ip, at the target's working depth e+0x2CEA. 07 §5.4 — NEW FACT. CODE BUG: the port
  pushes the unadvanced ip. Data: 2 uses, the target subs never return.
- **IMPL89 Q1** (in-place int ops on 10042–10044): read and write both go through the reference function (`*ref op=
  v`), so they use the instruction's own argument slot; 0 uses in the data. 07 §6 — NEW FACT. Port (getter read) is
  wrong but unreachable.
- **IMPL89 Q2** (op 155 without a card): cap +0x2638 is written always; `Spell_AddBonus` has no card test; all 19 uses
  follow op 122 in the same sub at the same time → unreachable. 07 op 155 row — NEW FACT. Port fine.
- **IMPL90 Q1** (held enemy tail): the op-128 update 0x42E010 is an unconditional call inside the shared tail
  (0x42DDA7), as are the boss HUD target and the ENEMY marker → a held enemy gets all of them. 07 §10.1 step 2 —
  SPEC FIXED. CODE BUG: the port skips the op-128 update for held enemies.
- **IMPL90 Q2** (op 178): `Ecl_OpMoveRand2` 0x4224A0 calls `Ecl_SetMode2Polar` 0x4222B0 like op 67. 05 §10 — SPEC FIXED. Port right.

## 3. CODE BUG list (port vs spec; all but 14–16 confirmed on scratch, see `re/work/runs/q32/q32_fixes.diff`)

1. HUD side panel repainted every frame; implement 12 §5.3 (dirty fields + writers, full-repaint counter 0x17CE8CC
   + its writers, VM-13 ip test, loop bounds, per-frame order, gauge only when power > 0, text rows only when dirty) and
   keep the back buffer between frames (present by copy; clears as in the draw log).
2. Score row (y 56) queued before the hiscore row (y 40); hiscore-continues digit stats+0x18 from score.dat (11 §5, 15 §2.3).
3. Small/large popups: draw through a VM with `Draw2DSnap`, **centred** (13 §4), small z 0.1; colour bytes in BGRA
   order (port swapped R/B of large popups); fade distance without origin correction; freed at age > 60 / > 90;
   large-popup x0 = pos.x − 3.5·(glyphs incl. '+').
4. Stage_DrawHigh: list-B effects and the B24 = 1 fade rect only while +0xB24 < 2 and no HUD cover (06 §10.2); the
   HUD cover condition also gates Stage_DrawLow steps 4/7.
5. ItemMgr_Draw resets sprite/colour every frame; use the +0x2D6 flip rule (08 §8.6).
6. Enemies: flags A bit 11 youkai colour2 missing (07 §10.1 step 7); enemy VMs drawn at z 0 instead of 0.25/0.3 (07 §10.6).
7. Ring effects: vertex z 0, not 0.07 (13 §2.5).
8. Ops 80/81 bit 1 do not switch the aura colour2 flag; op 174 lacks the interrupt (mode + 1) and the odd-slot spin
   negation (07 ops 80/81/174).
9. `Hud_Load` on a continuing stage re-creates the front VMs; keep them (12 §5.1a).
10. Op 158: start/end swapped and the fraction computed at draw time from the boss; store f32(a)/f32(max), f32(b)/f32(max)
    with the op enemy's max life (07 op 158).
11. Untextured HUD quads (`Screen_FillRect3`: boss bar, markers, remaining bars, dialogue box) logged/drawn 0.5 px off
    (12 §5.4).
12. Msg op 9 stage-clear VMs (+0x3778, +0x3CC0, 8 ghosts, capture after the next Present) and the 168 stage-start
    tiles not implemented (10 §9, 12 §5.1a, §5.4); the chain-count popup must be issued in the guarded part of Hud_DrawBoss.
13. Boss timer value: use HUD +0x24 from `Enemy_CheckTimeout` (12 §5.1); boss bar target = life / max life e+0x2E00,
    not e+0x2E04 (12 §5.1); remaining-bars bottom colour 0x202020 (12 §5.4).
14. Op 88: target ctx ip := its ip + size of that instruction before the push (07 §5.4) — spec only.
15. Held enemies must still run `Enemy_UpdateOrbitEffects` in the tail (07 §10.1 step 2) — spec only (no golden diff).
16. In-place int ops on non-int destinations must read the argument slot, not the getter (07 §6) — spec only, unreachable.
17. Tools: draw-log writer must print fog start/end like `%g`; the comparator should treat reference batches cut at
    682 quads (`P list 4`) as truncated (FORMAT.md §3).

Remaining golden draw differences after U (98.06 %): mass-popup frames truncated by the probe (most of ascii extra
62 k), billboard particle positions within ~0.05 px (etama#1 pos 108 k: D3DX projection rounding, not chased), a
few ascii glyph rows, enemy colour 3.6 k, stg1enm/stg2enm pos, etama.anm#4 2.3 k missing, text.anm 1/frame extra,
front missing in a few repaint frames. Not chased in RE-Q32.
