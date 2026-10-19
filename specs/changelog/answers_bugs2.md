# Answers — visual bugs round 2 (docs/bugs/VISUAL_BUGS_2.md, C1–C3)

Sources: static reading of th08.exe, the `ascii.anm`, `title01.anm` and `etama.anm` data, and wine runtime
captures. The refs are in `specs/data/ref/` and their names start with c1_, c2_ or c3_.

| item | verdict | where |
|---|---|---|
| C1 menu seam | **already specified** (filtering/addressing/snapping in 05 §12.4 and §9); it is an implementation/rendering issue. The exact draw recipe has been added | 12 §2.2 "What the top menu draws", 05 §12.4 "Consequence for snapped 2D sprites" |
| C2a Final-route notice | **spec gap** (fixed) | 12 §2.4 "Final-route notice" |
| C2b difficulty plate | **spec gap** (fixed) | 12 §2.4 "Difficulty plate" |
| C3a grey/white diamonds | **already specified** (13 §2.6 type 4–11, §3 enemy death/drops). The visual identity is now stated | 13 §3 row "(visual …)" |
| C3a pink glyphs | **already specified**: item type 7 time orb | 08 §8.2–8.6, answers_impl10 §2 |
| C3b gauge readout | **spec gap** (fixed) | 12 §5.9 |

## C1 — main-menu seam

What the original draws in that area (VERIFIED):
- The background `title00.png` is not a quad. Surface slot 0 is copied 1:1 to the back buffer with `CopyRects`
  (0x466BB0) every frame, before any sprite. It can never have a seam.
- Each menu item is one quad (`Draw2DSnap`, top-left anchored, 32 px tall, 28 px spacing, so neighbours overlap
  by 4 px).
  - Texture `title01.png` 512×512 A4R4G4B4. Practice Start selected = sprite 5, atlas (240, 0) 192×32 →
    UV (240/512, 0) – (432/512, 32/512), pos (64, 264).
  - SRCALPHA/INVSRCALPHA blend, alpha test ≥ 4, **LINEAR min/mag filter, WRAP addressing**, colour white.
- The item glow (white, alpha 17–68) continues past the sprite rect into the neighbouring atlas sprites (atlas
  rows 31 and 32 under Practice Start both have glow).
- The original runs at 640×480 and snaps quads to integer − 0.5. So every pixel samples a texel centre: LINEAR
  then equals point sampling and never reads row 32.

The port's line sits exactly on the bottom edge of the 192-px Practice Start quad (and faintly on the top edge of
Replay). That is the neighbour row being filtered in. Likely causes: rendering above 640×480 with LINEAR
filtering on the atlas, or a half-texel offset mismatch. Fix: render 2D at 640×480 and upscale the frame, or use
nearest filtering for snapped scale-1 quads, or inset UVs by half a texel. Ref: `c1_menu_practice_sel.png`
(original, 640×480).

## C2 — character select

**C2b difficulty plate.** On entering screens 5/9/13 (not spell practice 15), VM `131 + config+0x21` gets
interrupt 9 right after the range interrupt 8. Script label 9 slides it to (16, 384) over 20 frames (mode
out2) and keeps it visible with the selected sprite (Normal = sprite 136, 256×80: 三日月 / Normal Level /
description). Without interrupt 9 the range interrupt 8 hides it, which matches the port screenshot.

**C2a notice.** Drawn by 0x47052D, only on screen 5, from frame 9 on. It is chosen from the CLRD masks of the
cursor team and the current difficulty. Table and drawing details are in 12 §2.4. 「——FinalB 挑戦可能——」 =
sprite 145: the team has 6A cleared (any-clear bit 0x40) on some difficulty E–L and no no-continue 6B clear
(bit 0x80) on any. It is drawn at (400, 170), top-left anchored, white.

Refs, made with CLRD any-masks of team 0 poked to 0x41 at runtime:
- `c2_charsel_team0_normal_f40.png` (plate sliding in);
- `c2_charsel_team0_normal_finalB_notice.png` (settled, notice shown);
- `c2_charsel_team1_no_notice.png` (team 1 has no 6A clear, so no notice; plate still shown).

## C3 — in-game

**Pink glyphs** = time orbs, item type 7: script 0x3D + 7 = 68 (etama entry 1 script 43), sprite 179 (16×16, the
「永」 glyph on a purple glow). After 30 frames they fade to grey (128, 128, 128) at alpha 192 over 20 frames. The
"+1"/"+2" popups next to them come from the chain/single-kill paths (answers_impl10 §2: single kill = "+1" popup
plus one orb in mode 1; chain kill = "+n" popups plus c orbs in mode 3; boss part 2·n orbs). Further sources are
listed in 09 §4.5 / §9.2 / §9.3 and 08 §4.1 (bomb drop type 9).

**Grey/white diamonds** = enemy death / drop sparks, effect type 4:
- e+0x3311 is always 0, so `Effect_Spawn(4, …)`: ×4 at death plus ×3 at the drop.
- Script 36 = etama entry 1 script 11: sprite 198 (14×14 white four-point star), additive, alpha 192 → 0 and scale
  1 → 0 over 30 frames, velocity as in 13 §2.6.
- Runtime check (demo 0, frame 1400, probe `effscan`): groups of 7 type-4 effects of equal age, next to type-5
  hit sparks.

**Gauge readout** = 12 §5.9 (new). It is drawn by `Ascii_DrawPopups` at draw prio 14:
- bar `ascii.anm` sprite 155 at (32, 449) with 人/妖 ends (sprites 153/154; x from the gauge limits);
- a ▼ marker (152) at bar.x + 64 + g·56/10000, y 453;
- above it the percent text g/100 "%d.%02d%%" (two decimals at 0.8× scale, 2 px lower), centred at
  bar.x + 64 + g·40/10000, y 446;
- on the bar line the **max point value** (stats +0x24, e.g. "100790") from x = bar.x + 48, y = bar.y + 11;
- everything tinted by gauge zone (human max blue 0x7070FF, human 0xB0B0FF, youkai max 0xFF7070, youkai 0xFFB0B0,
  else white);
- it fades to alpha 64 while the player is in the bottom-left corner (y ≥ 400, x < 160) and slides/fades in at
  game start.

Refs: `c3_demo0_f1400_gauge_human_max.png` ("−100.00%" blue, 305000) and
`c3_demo0_f2200_gauge_youkai_max_timeorbs.png` ("82.76%" red, 306820, a time-orb shower with +1/+2 popups).
Demo 0 is Reimu & Yukari, Lunatic stage 5. The C3 screenshot's stage 1 is not available from the demos.

## Confidence
- VERIFIED: everything above except the cause of the port's C1 line. That is INFERRED from the original's
  sampling rules (05 §12.4), the atlas contents and the line's position. The exact port pipeline was not inspected.
- The diamonds = type-4 sparks: VERIFIED that these effects exist in bulk at runtime with these parameters.
  Matching them to the user's screenshot is visual (size, shape, soft white, around kills).
