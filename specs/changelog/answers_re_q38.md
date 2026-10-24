# RE-Q38 — SPEC-GAP batch 4: result screen and endings (2026-10-24)

Brief: docs/briefs/RE_Q38_BRIEF.md; markers: docs/specgaps/batch4_result_ending.txt (16) + docs/briefs/IMPL103_QUESTIONS.md
Q1. Runs: re/work/runs/q38/ (CHECKPOINT.md). Evidence: decompile (fn.sh) and disassembly (disx.py) of `Result_*`
0x454CB2–0x45A1F3, `Ending_DrawFade` 0x428910, `Player_UpdateBomb` 0x44C650, `Player_Die` 0x44AB40,
`Player_DyingUpdate` 0x44CBF0; the result00.anm dump (anm_dis.txt, VM 40/71 scripts); string tables read with
pe.py; callgraph.tsv for the `AnmMgr_TintOff` callers; the retail .end files for the fade arguments. No original run:
every answer follows directly from the code (the result-after-a-game screens are not in any existing draw log).

## Changed spec sections
| spec § | change |
|---|---|
| 06 §10.2 | `Player_UpdateBomb` turns the tint off at a normal-mode bomb **start**, not at the end; `Player_DyingUpdate` only at the real death; full caller list |
| 09 §6.1 | `Player_Die` (bombs ≥ 1): player VM colour2 = white with the colour's alpha + VM flag bit 17; ring effect pointer +0xE2B28 |
| 09 §6.2 | real death: ring effect ended, flag bit 17 cleared; the tint-off happens only there |
| 09 §7 | normal-mode start order: flag bit 17 off, ring effect ended, GameFlags bit 10 off, `AnmMgr_TintOff`, then index |
| 15 §2.12 | hidden code: exact counter rule (release reset, wanted key, wrong-key mask 0x160B, effect one call after the 10th key); effect = 13 story attempt counters of all 222 records, no cap, no checksum (QUIRK) |
| 16 §2.1 | state 0xF: interrupt 18 restarts VM 71 each frame 0–30 → panel fade-in runs frames 30–60 |
| 16 §2.2 | rank > 9: same-call finish (state 0xF, all VMs interrupt 2 over the d + 3, +0x58 := name, LSNM, no SE); +0x58 := "" at frame 0 |
| 16 §2.4 | page input order (cursor → D → cancel → confirm; top cancel+confirm exits); state 3 code after the page input; text-VM initial colour and why it is never seen |
| 16 §2.6 | state-12 headers via `Replay_Load` (decoded copy; failed load = empty, entry kept); header colour rule; table 0x4C8010 contents |

## Markers
| port marker | answer | spec § | verdict |
|---|---|---|---|
| src/result.rs:152 (text-VM initial alpha) | `AnmVm_Init` colour 0xFFFFFFFF (alpha 255), visible; never observable: drawn only in state 8 (rendered at frame 10 = first visible frame) and 0x13–0x15 (alpha written every frame) | 16 §2.4 | right (equivalent) |
| src/result.rs:329 (hidden code vs page input) | page input first; a confirm/cancel call returns before the code step | 16 §2.4, 15 §2.12 | **CODE BUG** minor |
| src/result.rs:454 (D vs cursor/confirm) | cursor → D → cancel → confirm; the dump does not depend on the cursor | 16 §2.4 | right |
| src/result.rs:579 (CursorUpDown2 SE) | SE 12 (already VERIFIED RE-Q24) | 16 §2.4 | right (stale marker) |
| src/result.rs:888 (VM 71 interrupt 18) | `Result_EndBanner` writes it every state-0xF call with frame ≤ 30; each restarts the fade, so the panel fades in over frames 30–60 | 16 §2.1 | **CODE BUG** (visual: port sends once, opaque at frame 30) |
| src/result.rs:949 (state-9 slide-in) | interrupt d + 3 to all 72 VMs at frame 0 before the insert (already in 16 §2.2); rank > 9 overwrites it with interrupt 2 in the same call and copies LSNM into +0x58 | 16 §2.2 | rank ≤ 9 right; **CODE BUG** minor for rank > 9 (no interrupt 2, replay name not LSNM) |
| src/result.rs:1041 (header colour, shot table) | header colour = leftover: white in 10/11/12/14; in 13 the last grid cell's (0xC0C0C0C0, 0xFFFFFFC0 on END). 0x4C8010 = same strings as 0x4C86C0 | 16 §2.6 | table right; **CODE BUG** minor (state-13 header colour) |
| src/result.rs:1303 (pos_offset added by the owner) | VERIFIED disasm 0x458764: `Vec3_AddInPlace(pos, +0x288)` per VM, pos restored after the draw | 05 §2, 16 §2.4 | right (stale marker) |
| src/result.rs:1335 (slot info fields) | headers come from `Replay_Load` (decoded), first 0x134 bytes; a failed load counts as empty | 16 §2.6 | decode right; **CODE BUG** minor (port treats a raw-magic file that fails to load as valid → overwrite question) |
| src/result.rs:1386 (wrong key / release) | release of Focus+Skip → 0; wrong key = edge on 0x160B (Shot, Bomb, ESC, Q, S, Enter) → 0; arrows/D never reset; effect one call after the 10th key | 15 §2.12 | **CODE BUG** |
| src/result.rs:1406 (attempt counter) | all 13 story attempt counters (+0x124) of all 222 records +1; no magic test, no cap, no checksum refresh | 15 §2.12 | **CODE BUG** |
| src/ending.rs:326 (script end without `@z`) | no end test; every retail file ends `@z` | 16 §1.3 | right (unreachable) |
| src/ending.rs:425 (16 VMs) | 16 VMs constructed, VM 15 never run/drawn; the `@R` write to it is unobservable | 16 §1.2, §1.3 | right |
| src/ending.rs:446 (> 15 lines) | index 15 = invisible VM 15, ≥ 16 corrupts; dropping is allowed; retail ≤ 4 lines | 16 §1.3 | right |
| src/ending.rs:467 (fade counter order) | colour from t, then t += 1 (VERIFIED RE-Q26). The port's n.max(1) differs only for n = 0, which no retail file uses (180, 240, 30000) | 16 §1.5 | right (stale marker) |
| src/ending.rs:554 (picture offset) | ftol truncation, whole pixels (VERIFIED RE-Q26) | 16 §1.5 | right (stale marker) |
| IMPL103 Q1 (tint at a normal bomb start) | real: VM bit 17 off, ring ended, bit 10 off, then `AnmMgr_TintOff`, then the index. The bomb end has no tint-off; `Player_DyingUpdate` only at the real death | 09 §7, 06 §10.2 | **CODE BUG** (no tint-off at the start; extra tint-off at the bomb end and on every dying frame) |

## New findings outside the markers
- All result menu pages test cancel before confirm (16 §2.4); the port's `page()` tests confirm first → **CODE BUG** minor (both on the same frame).
- `Player_Die` white silhouette (colour2 + VM flag bit 17, 09 §6.1) is not modelled in the port → **CODE BUG** minor (visible only when the player VM colour is not white).
