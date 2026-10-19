# Answers to IMPL13_QUESTIONS (dirty room, 2026-10-18)

All answers were read from th08.exe (`re/extracted/all_decomp.c`, byte scans of the image where noted).
Spec sections fixed in place: 12 §5.9, 11 §7 (exit/fresh-start flags), 06 §8 and
`specs/data/drawlog/FORMAT.md` §2 (stage fog, task deliverable 2), 15 §1.1 (cfg flag bit 10).

## 1. Setup order in `GameManager_StageStart` — VERIFIED

`Ascii_SetupGaugeVms` runs **after** `Player_Load`, so the readout always uses the limits of the game that is
starting. The port's order is correct.

- `Player_Register(0)` is the first registration in `GameManager_StageStart` (0x43ABD7). It is in both branches
  (continuing stage and fresh start). It adds the player calc node with `FuncChain_AddCalc` (0x43C880). That
  function calls the node's on-register callback (`Player_Load` 0x44D650) **synchronously** before linking the
  node. So the gauge limits 0x164D300/0x164D302 (and the other limit words 0x164D304..30A) are written right there.
- Much later, after Stage, Bullet, Enemy, Effect, Hud and Gui are registered (and the replay), StageStart calls
  `Ascii_InitVms` then `Ascii_SetupGaugeVms`.
- Detail: `Player_Load` sends gauge interrupt 1 (fresh start) **before** it writes the limits. That does not
  matter: the setup restarts the scripts afterwards and sends the stored id (1) again.

## 2. Which exits hide the readout (interrupt 99) — VERIFIED

Retry is **10**. A retry hides the readout (99). The new game is then a fresh start, so `Player_Load` sends 1
and the readout fades in again: G0 slides up over 40 frames, and G1–G3 fade in from frame 60.

The two flags are computed from the requested scene (0x17CE8B4) at the moment the GM is removed or registered:

| flag | written by | value |
|---|---|---|
| 0x17CE8C0 "full exit" | `GameMgr_OnRemove` 0x43BE2C, first thing | 0 if requested ∈ {3, 11, 12}, else 1 |
| 0x17CE8BC "fresh start" | `GameMgr_OnRegister` 0x43AAF4, first thing | 0 if requested ∈ {3, 11, 12}, else 1 |

`Supervisor_Calc` (11 §7) writes requested = 3 **before** it unregisters the GM for requests 11 and 12. So the
GM callbacks see:

| request | requested seen by OnRemove / OnRegister | full exit → 99 | fresh start → 1 |
|---|---|---|---|
| 3 next stage | 3 | no | no |
| 11 reload same stage (pause / continue menu) | 3 | no | no |
| 12 advance (never requested) | 3 | no | no |
| 10 retry (pause / game over) | 10 | yes | yes |
| 1, 6, 7, 9 (menu, name entry, ending, …) | that value | yes | — (no new GM) |

Interrupt 99 is sent by `Player_OnRemove` 0x44DC60 when the full-exit flag is set. When the flag is 0 (3/11), the stored
id (1, 2 or 3) survives. The next `Ascii_SetupGaugeVms` restarts the scripts and sends it again.
The fresh-start flag also controls the rest of the StageStart fresh path (11 §3). On that path StageStart also
tests requested == 10 itself and then bumps two more score-data counters (0x164CD6C + 0x44·difficulty and
0x164CF04, `FUN_0043BE09`; INFERRED: retry counts).

The old 12 §5.9 wording ("next stage / retry (requested scene 3/11/12)") was wrong. 11 means reload and 10 means
retry; the 11 §7 table was already right. Fixed in 12 §5.9.

## 3. Limits seen by `MainMenu_OnRegister` — VERIFIED

They are the limits of the **last game played** (any shot type, including demos). Before the first game they are
**0/0**. Either way nothing visible depends on them, so the port's default team limits are fine.

- Only writer: `Player_Load`. A byte scan for the addresses finds 4 stores to 0x164D300 and 3 to 0x164D302,
  all in `Player_Load` (0x44D9F1..0x44DAD0). The only other references are the two reads in
  `Ascii_SetupGaugeVms` (0x40270C, 0x402737). Nothing else resets the GM block at that offset.
- 0x164D300 lies in the uninitialised part of `.data` (beyond the raw data), so it is 0 at process start.
  The boot setups (`AsciiManager_Load`, the first `MainMenu_OnRegister`) therefore place both 人 and 妖 at
  x = 88 (the script position, no offset).
- Hidden in the menu: at boot the stored id (+0x827C) is 0. Interrupt 0 has no label in scripts 5–8, so the
  VMs stay in `hide_wait_interrupt`. After a game the id is 99, which jumps to the hide label (−1).
  The next `GameManager_StageStart` runs the setup again with the new limits before anything is shown (Q1).
- So a menu-time setup with any limits is invisible. The only state it leaves behind is G0/G3's last position.
  Neither the restart nor the setup resets that position, and the label-1 script sets it again anyway.

## Extra findings while checking (spec fixes)

- `Ascii_SetupGaugeVms` also restarts five more ASCII-manager VMs: +0x1520 with script 9 (sprite 156, pulsing
  scale) and +0x17C4/+0x1A68/+0x1D0C/+0x1FB0 with script 10 (sprite 157, fade in on interrupt 1, out on 2).
  `Player_Load` sends interrupt 2 to the first three of those (helper 0x422BB0), and `Player_OnRemove` sends 99.
  They are drawn by `Ascii_DrawStrings` and the HUD (`Hud_Update`/`Hud_DrawPanel`). Their role belongs to the HUD
  spec. Recorded in 12 §5.9 as a side effect of the setup (INFERRED role, VERIFIED calls).
- G1/G2 positions are not cumulative. `AnmFile_SetScript` → `AnmFile_StartScript` (0x45E580) runs the VM once
  immediately (`AnmVm_Run`). Scripts 6/7 set pos (88, 449) at time 0 before `hide_wait_interrupt`. So the
  setup's x offset is always applied to 88.

## 4. Stage fog depth (deliverable 2) — VERIFIED (static + runtime against wine screenshots)

Static (th08.exe):
- Device init 0x442F60: FOGTABLEMODE = NONE, FOGVERTEXMODE = LINEAR, FOGDENSITY = 1.0 (unused by linear),
  FOGCOLOR 0xFFA0A0A0, FOGSTART 1000, FOGEND 5000. FOGENABLE = 1 unless cfg+0x38 (Supervisor+0x150) **bit 10** is set.
  The same bit gates every `Supervisor_FogOn`/`FogOff` (0x4481DF/0x448229) call, so with bit 10 set fog is never on.
- RANGEFOGENABLE is never set. The byte scan for `call [reg+0xC8]` (IDirect3DDevice8::SetRenderState) finds 36
  sites in game code. All are accounted for: none sets state 48. So D3D's default (off) applies, and the fog
  distance is the eye-space **z**, not the radial distance.
- Fog start/end/colour come only from `Stage_DrawLow`: start = stage +0xAEC, end = +0xAF0, colour = +0xAF4
  (tinted when the global ANM tint is on). They are written as floats every frame before the layers. `Stage_DrawHigh`
  turns fog off and sets start/end to 1000/2000.
- Fog is enabled only around `AnmManager_Draw3DWorld` in `Stage_DrawLayer`. Billboards and ribbons turn it off.
  Draw3DWorld submits **untransformed** vertices (FVF 0x102 / 0x142) through the world, view and projection matrices,
  so the fixed-function pipeline computes the fog itself.
- The stage projection is PerspectiveFovLH(near 30, far 1800), so w_clip = eye z. The eye depth is d = 1/rhw, and
  also d = 30·1800 / (1800 − 1770·z) from the projected depth z.

Runtime (wine reference screenshots, `re/tools/drawlog.py render --fog`, playfield only):

| demo frame | no fog | factor clamped per vertex | **depth interpolated, clamped per pixel** |
|---|---|---|---|
| d0 f1400 | 6.74 | 4.41 | **0.27** |
| d0 f2200 | 6.06 | 3.35 | **0.30** |
| d1 f1000 | 25.15 | 0.95 | **0.26** |
| d2 f1000 | 6.75 | 1.00 | **0.40** |
| d3 f1000 | 3.84 | 0.66 | **0.22** |
| d3 f4000 | 5.35 | 0.55 | **0.20** |

(mean |Δ| per channel, 0–255; all three columns with near/far clipping)

The remaining 0.2–0.4 is filtering and the few pixels near sprite edges. It is the same level as the menu frames
that have no 3D. The model that matches:
1. Clip each triangle to the near and far planes (0 ≤ z ≤ w) in homogeneous space.
2. Interpolate the eye depth d perspective-correctly across the triangle. Because w = d, this is the true eye z
   of the pixel.
3. f = clamp((end − d)/(end − start), 0, 1) **per pixel**, then colour = f·colour + (1 − f)·fog colour. Alpha
   is not changed.

Clamping per vertex before interpolation (what the D3D8 documentation describes for vertex fog) over-fogs triangles
whose vertices lie on both sides of `start`. The walls of stage 5 in d0 f1400 come out 10–12 levels too dark that
way, and they are exact with the per-pixel clamp. The reference logs and screenshots come from wine, so the port should use
the per-pixel model. Real Windows drivers may differ slightly in such triangles (INFERRED, not testable here).

Log precision note: the draw log rounds rhw to 4 decimals (0.0013 ≈ ±4 % of 1/rhw at d ≈ 770). That alone moves
the fog factor by up to ~0.1 in the 560–880 range. Use the logged z with the formula above when deriving d from a log. z also has
4 decimals, but d is much less sensitive to it (±1 at d ≈ 740, ±3 at d ≈ 1400).
