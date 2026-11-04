# RE-Q68 — stage 5/6 3D background, boss portrait tint, worst dl3 recordings (2026-10-31)

Runs: re/work/runs/q68 (CHECKPOINT.md). Scratch port = q67 portcopy (RE-Q66 + RE-Q67
fixes) + RE-Q68 A behind `Q68NOA` and the draw-log tool rules behind `Q68NOTOOL` (re/work/runs/q68/portcopy,
target/re68). Before = target/drawgroups_dl3_145 / target/drawgroups_145 (master 125c751, IMPL145).
Session cuts with `--to` (menu-relative rows, ≈ present + 2400 in stage 5/6) via re/work/runs/q68/rn.sh.

## Changed spec sections
| section | change | evidence |
|---|---|---|
| 12 §5.8 (Spell_Draw) | `Spell_Draw` has exactly one tint call, `AnmMgr_TintOn(+0x2620)` in the VM 7 block (0x417A53); it never turns on the 0xFFF01010 freeze tint, so during the death freeze (GameFlags bit 10, already turned off again by `BulletMgr_Draw`) the player / boss cut-ins are drawn untinted | disassembly 0x4178C0 (one `call 0x40BAD0`); callgraph: TintOn callers = BulletMgr_Draw, 0x42EB90, Spell_Draw, Stage_DrawLow; ref500 50948 present 76517 (ref c0ffffff, port ffff1f1f), 54840 59454 (23ffffff / 45ff1f1f), dl3 udTh03 55198 (39ffffff / 71ff1f1f) |
| 06 §11.2 (3D world quad) | the pipeline clips in homogeneous space; a vertex in front of the near plane or on/behind the camera plane matters only through its clip coordinates; its projected screen value is unstable (f32 eye-z rounding ≈ 3·10⁻⁵ at w ≈ 0.002 moves it by millions of px or flips its sign); the hook writes centre / z = rhw = 0 for w = 0 | dl3 54440 presents 44000–49882 and 44000–~59000, udSs12 to ~78600: every differing stg5bg / stg6bg vertex agrees with the reference in clip space (example 54440 47898: ref w −0.0023498, port −0.0023193, clip x −380.5 both) |

## Causes
| cause | groups / recordings / first present | original | spec | port |
|---|---|---|---|---|
| A freeze tint on cut-ins | face_st05 / face_st04a / face_st04b #0 [1,1,255,511] colour (ref500 50948 76517 ×25, 54840 59454 ×28, dl3 udTh03 55198 ×12) | `Spell_Draw` never sets 0xFFF01010; the port's Spell draw sets it while GameFlags bit 10 → c0ffffff × f01010 / 0x80 (saturated) = ffff1f1f | 12 §5.8, 06 §10.2 | CODE BUG |
| B near-plane vertices of 3D stage quads | stg5bg.anm#0 [257,257] / [257,1] / [481,1] / [1,1] pos+z/rhw, missing / extra (24 dl3 recordings, from 54440 45114 / 47898), stg6bg [1,1,253,253] (12, udSs12 77534), stg7bg (12) | D3D clips at 0 ≤ z ≤ w; the projected values of w ≈ 0 vertices are numerically meaningless | 06 §11.2 | port right — TOOL (compare in clip space) |
| — worst dl3 53191 / 83085 / 51670 / 54214 | etama#1 [160,224] pos / pos+colour (6–9k frames each) | op-128 orbit stars | 13 §2.6c | precision (skipped per brief) |
| — same recordings | ascii [0,112] / [80,112] pos every 5th present (slide x crosses .5: 14 vs 15 px wide glyphs) | 0.9-scaled glyph `Draw2DSnap` ties | 05 §10 | precision (skipped) |
| — same | etama#1 [65,241,79,255] / [193,113,255,175] (1–7 frames: 1-px snap tie of a shrinking particle, 0.06 px offsets) | | 13 §2.6c | precision |
| — same | ascii [0,208] / [104,208] missing, loader race at present 244 (83085 `C z`, ascii [0,48]) | | RE-Q67 B, 12 §5.9 | already explained |
| — other dl3 | ascii [0,0,8,8] colour ±2 alpha / 1-px (12 recordings, 1–5 frames); udSs12 74264 one frame stage tint 51 vs 4f; 78684 9771 nowloading vs fps text | | | precision / loader race / FPS digits (skipped) |

Result for Q3: after RE-Q66/Q67 the four worst dl3 recordings contain no non-skipped cause; their percentage is the orbit-star
class.

## CODE BUG list (for the next IMPL brief)
| id | src function | change | spec |
|---|---|---|---|
| A | `src/gui.rs` Spell manager `draw` (`Spell_Draw` 0x4178C0) | delete the `if self.player.borrow().freeze_bullets() { (anm.tint, anm.tint_on) = (0xFFF01010, true); }` block at the top; the only tint in this function stays the +0x2620 tint of step 4 | 12 §5.8, 06 §10.2 |

## TOOL changes (`src/drawlog.rs`, draw compare used by `session` / `drawdiff`; scratch versions behind `Q68NOTOOL`)
1. `compare()`: for quads with `ztest=le`, a vertex with z < 0 or rhw ≤ 0 on either side is compared in clip space:
   w = 1/rhw (0 if rhw = 0), nx = (x − vp.x)/(vp.w/2) − 1, ny = 1 − (y − vp.y)/(vp.h/2), h = (nx·w, ny·w, z·w, w);
   tolerance 1e-3 + 2e-4 · (max |h| component over both quads, each capped at 1e4) + 1e-4 · w² · (1 + |nx| + |ny|)
   (the last term = two 4-decimal roundings of the logged rhw); if either rhw is 0 only w is compared. uv / colour as
   usual; no z/rhw bit for those vertices. Other vertices keep the existing rules.
2. `items()`: the matching centre ignores vertices with rhw ≤ 0 (besides z ∉ [0, 1]); `off_vp` is not applied to a quad
   with any vertex with z < 0 or rhw ≤ 0 (its screen bbox is not its visible extent; a port w of +1.5e-5 vs a reference
   −1.5e-5 flips the bbox out of the viewport and dropped the port quad → "missing").

## Effect (scratch, trace identical from the first main-menu row in every run)
| recording / cut | before | after | note |
|---|---|---|---|
| ref500 silentselene_50948 (to present ~76.9k) | 77990 / 78940 (Q68NOA) | 78015 / 78940 | face_st05 colour 25 → 0 |
| ref500 silentselene_54840 (to ~60k) | 61943 / 62082 | 61971 / 62082 | face_st04b colour 28 → 0 |
| dl3 gensokyou_th8_udTh03 (to ~55.8k) | 55787 / 55818 | 55799 / 55818 | face_st04a colour 12 → 0 |
| dl3 silentselene_54440 presents 44000–49882 (drawdiff on the a0 log) | stg5bg missing 5, extra 2, changed 5 | 0 / 0 / 0 | TOOL |
| dl3 silentselene_54440 (to ~59.3k, c0 log, drawdiff Q68NOTOOL vs session) | stg5bg missing 5, extra 2, changed 55 (pos/z) | 0 / 0 / 0 | TOOL |
| dl3 gensokyou_th8_udSs12 (to ~80k, same) | stg5bg 5 / 2 / 70 (pos/z) + 25 colour; stg6bg changed 13 | stg5bg 25 colour only (one frame 74264, ±2 stage tint); stg6bg 0 | TOOL |

## Full-set compare (orchestrator)
After the IMPL cycle with A and the TOOL changes on master: `target/longcheck.sh <IMPL-tag>` (ref500 + ref_b2 logic
batches, drawgroups on the batch500 draw-log set + golden and on dl3, METRICS row). Expected: all stg5bg / stg6bg / stg7bg
pos+z/rhw and missing / extra groups gone (24 / 12 / 12 dl3 recordings, probably also the batch500 stg5bg extras such as 50948
65228, not checked); face_st05 / face_st04a / face_st04b colour groups gone; logic batches unchanged (A and the tool are draw-only).
