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
| B | `-` (boss bar) | op 158 word 1 = start, word 2 = end, fractions of the op enemy's max life; bar quads at exact D3D coords | 07 op 158, 12 §5.1 / §5.4 — SPEC FIXED | 96.85 % / 4270 |

Tool fact (FORMAT.md §3): the probe logs at most 4096 vertices per draw call, so reference batches > 682 quads are
truncated (`P list 4`); most remaining ascii "extra" quads in pause frames after mass cancels are this artifact.

## 2. Questions

(filled below)

## 3. CODE BUG list

(filled at the end)
