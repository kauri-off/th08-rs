# RE-Q64 — draw-log differences, part 5 (2026-10-30)

Brief: docs/briefs/RE_Q64_BRIEF.md. Runs: re/work/runs/q64 (CHECKPOINT.md; scratch port = q63 portcopy + Q64 fixes behind
`Q64NO*` env switches, target/re64). Recordings: ref500 draw-log set (re/work/runs/batch500/drawlog.txt) + golden.

## Changed spec sections
| section | change | evidence |
|---|---|---|
| 13 §5, 06 §10.2 | spell-background VMs are drawn with `AnmManager_DrawRotFlush` 0x40BAF0 (= Draw2DRot, mode 2) whatever their draw mode, then the mask callback; writers of the first script +0xB34 listed (EffectMgr 0, Msg entry 0, StageStart in spell practice) | 0x409640, 0x43ABD7; udSi123 / udI811 present 306 (eff04a/eff05 #1 rotating −π/400 per frame) |
| 11 §3.1 | spell-practice spell-background base 2: exact card-id exceptions per stage (6A 0x77–0x7A, 6B 0x93–0x96, Ex 0xBF–0xC1 and 0xD5) | 0x43ABD7; udSr22 / udGu02 / udKe02 present 306 (eff06/eff08 scripts 2/3) |
| 07 §9.3, op table 55/56/59/60 | the whole `Enemy_AutoFireAndDirAnim` runs only while life > 0; the direction-script ops store through 0x421DE0, which also sets the current direction e+0x332E := −1 → the next call restarts the idle script (pos offset kept) | 0x421DE0, 0x423150; spell-practice boss bob phase present 306 |
| 12 §5.1a, 02 §4 table | `Hud_Load` in spell practice: stgNtxt file = stg8txt.anm for card ≥ 0xCD; only VM 0 := script 3 with sprite (0x4C7670 record +0xC) + 3, only when not reloading the same stage or the record flag is set; +0xC values listed | 0x4390EE, table 0x4C7670; udSr22 / udGu02 / udKe02 / udI811 present 413–414 |
| 07 §12.1 | mask copy colour 0xFFE0C0C0 spelled out as D3DCOLOR (R E0, B C0) | udSi123 present 396 (`ffe0c0c0`) |
| 08 §6.1 | laser emitter VM +0x2A4 = `AnmVm_InitWithSprite` (sprite 0x92 + table, no script) | 0x430F20; udI811 / udSr22 etama sprite 147 at each laser origin |
| 14 §2.11 | Youmu slash ring: minor axis 192f + 0.0001 (never 0), f = (x²)² | `EffUpd_Slash` 0x413070, consts 0x4B4440/0x4B456C; 54840 present 316 |
| 06 §11.3 | ribbons get no screen-shake offset | 0x464DD0 / PushQuad; 53406 present 4679 (player sprite shakes ±3.98, ribbons steady) |
| 13 §4 | `Ascii_Calc` reads GameFlags bit 10 live (calc prio 1) | 0x402200; udFa11 present 7094 (two static boss popups kept) |
| data/drawlog/README | probe logs `asciicalc C` at `loader gauge` | re/probe/session.inc |

Already in the specs (no change, port bug only): orbs collected after `Spell_End` still change +0xFC (07 §11.1) —
udLi14 7299, 51890 7835, 53020 6942, udFn15 7176 (bonus line rises by 8000 per orb in the reference).

## Effect (scratch draw-log compare; trace identical in every run)
Before = q63/fix (= RE-Q63 after), after = q64/fix2 (bin_fix2: Q1a–f, Q2a–b, Q3a–b, Q1c with the life gate, and the
`edge_vp` tool rule). Table filled from `sum.py base fix2`; numbers are the first differing present.

| recording | before (q63 fix) | after (q64 fix2) | next first difference |
|---|---|---|---|
| ne_52233 | 3826 etama#1 pos 1 | 3826 | 13 §2.6c accepted (skip) |
| ne_54723 | 215 ascii#0 pos 7 | 215 | gauge loader race (RE-Q63 Q1b; new probe field `asciicalc`) |
| ne_50948 | 3462 etama#1 pos 1 | 3462 | etama#1 pos 1 |
| ne_79090 | 1900 etama#1 missing 1 ¹ | 5021 | ascii#0 uv 1 / colour 2 in a truncated ref frame ² |
| ne_54840 | 316 etama#2 missing/extra 36 | 2171 | etama#1 pos 1 |
| 8_udSe43 | 3567 etama#1 extra 1 ¹ | 4674 | ascii#0 pos/uv/colour 2 in a truncated ref frame ² |
| ne_51890 | 7835 ascii#0 extra 3 | 10160 | ascii#0 pos/uv/colour 1 (ref truncated 327) |
| 8_udLi14 | 7299 ascii#0 uv 2 | 15470 | face_st08#0 missing 1 / face_st08m#0 extra 1 |
| ne_53406 | 4679 stg8bg pos 47 | 7825 | ascii#0 pos/uv/colour 1 (ref truncated 728) |
| ne_53020 | 6942 ascii#0 extra 2 | 15778 | etama#1 z/rhw 1 |
| 8_udVa11 | 1286 etama#1 pos 2 | 1286 | etama#1 pos 2 |
| 8_uddo18 | 1487 etama#1 pos 1 | 1487 | etama#1 pos 1 |
| 8_udFn15 | 7176 ascii#0 uv 2 | 7504 | etama#1 pos 1 |
| _udSi123 | 306 eff04a#1 pos 1 | 3395 | etama#1 missing 7 / extra 6 / changed 15 (pos, colour, z/rhw) |
| 8_udg342 | 213 ascii#0 missing 3 | 213 | gauge loader race (RE-Q63 Q1b) |
| 8_udKe02 | 299 eff06#0 extra 1 | 1983 | scene 1 frame 1: `-` screen quad missing 1 (th08logo.jpg) |
| 8_udI811 | 305 eff05#0 pos 1 | 2187 | etama#2 extra 22 / changed 32 (sprite 16×12 vs 16×24) |
| 8_udGu02 | 306 eff08#0 extra 1 | 4506 | ascii#0 extra 7 |
| 8_udFa11 | 7094 ascii#0 missing 6 | 7094 | etama#1 missing 1 (ascii popups now match) |
| 8_udSr22 | 306 eff06#0 extra 1 | 2302 | etama#1 pos 1 |
| golden (--to 8000) | 4439 | 4439 | 13 §2.6c accepted; identical frames 7940 → 7996 |

Trace identical (T=) in all 20 + golden. Identical frames over the 20: 671 961 → 895 316 (part of it is the
`edge_vp` tool rule).
¹ q63/fix gave 5021 / 4674 after re-comparing with the final RE-Q63 `off_vp` margin; the q63 batch logs show 1900 / 3567.
² The ref frame has the 682-quad cut (FORMAT.md §3); the changed quads are popups beyond the cut, matched across the cut
boundary. Not investigated (next part).

Next differences (not investigated in RE-Q64): udSi123 3395 etama#1 group; udI811 2187 etama#2 sprite height;
udGu02 4506 ascii#0 extra 7; udKe02 1983 title-logo screen quad; udLi14 15470 portrait file face_st08 vs face_st08m;
53020 15778 etama#1 z/rhw; 79090 / udSe43 / 51890 / 53406 ascii#0 in truncated ref frames; single etama#1 pos 1
rows (50948, 54840, udFn15, udSr22, udVa11, uddo18).

## CODE BUG list (src function → change → spec)
- Q1a `src/stage.rs` `Stage` draw (Stage_DrawHigh part, `for vm in &mut self.spell_vms`): draw each spell VM with
  `anm.draw_mode(gfx, vm, 2)` (Draw2DRot), not `anm.draw` (13 §5, 06 §10.2).
- Q1b `src/gamemgr.rs` `stage_start`: after the subsystems register, in spell practice set `stage.spell_first = 2` for
  stage idx 6 unless card 0x77–0x7A, idx 7 unless 0x93–0x96, idx 8 unless 0xBF–0xC1 or 0xD5 (11 §3.1, 13 §5).
- Q1c `src/eclvm.rs` ops 55/59 and 56/60: also set `e.dir = -1`, **and** in `autofire_and_dir_anim` skip the direction
  animation while life ≤ 0 like auto-fire (07 §9.3). Both are needed: the reset alone restarts a dead stage-1 boss's
  idle script (golden present 4433 stg1enm sprite wrong); with the life gate golden is back to the accepted 4439.
- Q1d `src/hud.rs` `Hud::load`: spell practice with card ≥ 0xCD loads `stg8txt.anm`; title VMs in spell practice: VM 0 :=
  script 3 + `set_sprite(table +0xC + 3)` only when `!reload_same || spell_bgm_restarts(card)`, VMs 1–3 untouched
  (12 §5.1a, 02 §4 table: 0,1,0,1,0,1,1,1,1,0,1,2,0,1,2,0,1,2).
- Q1e `src/stage.rs` `draw_spell_mask`: `vm.color` is B,G,R,A — use `0xFFE0C0C0u32.to_le_bytes()` as is (07 §12.1).
- Q1f `src/bullet.rs` laser spawn: emitter = `AnmVm::default()` + `init()` + `anm_slot` + `set_sprite(0x92 + n)`, no
  `set_script` (08 §6.1).
- Q2a `src/effect.rs` Youmu slash 0x30 update: `amp = 192·f + 0.0001` (14 §2.11).
- Q2b `src/stage.rs` ribbon draw: no `anm.screen_offset` on the vertices (06 §11.3).
- Q3a `src/enemy.rs` `spell_add_bonus` + `src/gui.rs` bonus copy: without an active card still add n to the bonus +0xFC
  (cap +0x2638, unless bit 11) and keep showing it (07 §11.1); the bonus state must outlive `Spell_End`.
- Q3b `src/ascii.rs` `Ascii::calc`: popup freeze test reads `gamemgr::flags() & F_DEATH` live, not `effect.frozen` (13 §4).
- TOOL `src/drawlog.rs`: new `edge_vp(vp, q)`: an unmatched quad whose bounding box reaches at most 0.25 px past a
  viewport edge (x1 ≤ vx + 0.25, y1 ≤ vy + 0.25, x0 > vx + vw − 1.25 or y0 > vy + vh − 1.25) is counted neither missing
  nor extra; the `off_vp` 0.05 px margin stays (05 §9 step 3; 2-decimal log rounding moved a stage-1 quad from 16.06 to
  16.05 between builds: 52233 present 577, 50948 present 3016). Scratch version behind `Q64NOTOOL` in
  re/work/runs/q64/portcopy (its "0.1 px band" comment above `off_vp` is stale: the margin is 0.05).
