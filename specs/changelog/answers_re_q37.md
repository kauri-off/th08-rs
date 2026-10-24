# RE-Q37 — SPEC-GAP batch 3: menus (2026-10-24)

Brief: docs/briefs/RE_Q37_BRIEF.md; markers: docs/specgaps/batch3_menus.txt (17). Runs: re/work/runs/q37/
(CHECKPOINT.md; `sess_top` = copy of sessions/options_cfg with crafted polls after the return to the top menu,
replayed in the original off-screen → `play_top`). Evidence: decompile (fn.sh), the title01 anm dump, the
`options_cfg` reference trace, and the `play_top` run.

## Changed spec sections
| spec § | change |
|---|---|
| 00 §2 | leaving the message loop; `Frame_Run` 2 (calc/draw return 5) = full restart of window, device, sound, ANM manager and Supervisor; log buffer, error MessageBox, log.txt |
| 12 §2 intro | option item 7 switches in call P + 29 (in-place `MainMenu_Option` call increments the counters); replay screen input in call numbers; top menu → §2.2 |
| 12 §2.1 | VM 43 belongs to the option screen; key config = VMs 44–76; VM 141 slide and the interpolation exception |
| 12 §2.2 | item 7 row; complete list of +0xC2BC writers (difficulty / replay entry := 0, option and key-config descriptions and their strings); top-menu input timing (cursor from call 9, confirm/cancel from call 19) |
| 12 §2.3 | Extra entry (0x43C317 = 0 → cursor 0); entry block order before the +0x6C forward |
| 12 §2.4 | cancel stores g_ShotType := cursor; screen 15 cancel (SE twice, interrupt 6, 30 frames) |
| 15 §1.3 | 0x4C868C written every call (32 = none, static, never reset); key config VM layout; idle exit counter +0xC294 rule; return 5 → 00 §2 |
| 15 §3.8 | Left/Right paging only for > 15 files and wrapping; entry block; the no-stage error path |
| 16 §4 | `Info_Show` iteration order, rect (0,0)–(639,479), fade-in ends at 10, frozen previous input, 179 < t < 5980 |

## Markers
| port marker | answer | spec § | verdict |
|---|---|---|---|
| src/menu.rs:136 (Info t += 1) | per iteration: draw t → poll → Present → test → t := 5980 → t += 1; the jump shows one iteration after the press, as t = 5981 | 16 §4 | **CODE BUG** (port jumps on the press frame and draws 5980; also edge test vs the frozen pre-loop word and the t < 5980 bound; quad 639×479) |
| src/menu.rs:196 (fade-in end) | the formula is right; the spec's "falls to 23" was wrong: t = 19 gives 10 | 16 §4 | right (spec fixed) |
| src/menu.rs:609 (+0x6C forward) | the whole entry block runs first, then the forward to 9, return 1 without counter increments | 12 §2.3 | right |
| src/menu.rs:759 (screen-15 cancel interrupt) | range interrupt 6, SE 11 twice (one sound), +0x10 > 29 → screen 0, cursor 2. **New**: every character-screen cancel stores g_ShotType := cursor | 12 §2.4 | interrupt right; **CODE BUG** reachable (shot not stored on cancel) |
| src/menu.rs:803 (option idle exit) | counter +0xC294, reset by g_InputRawCur ≠ 0 only from +0x1442C ≥ 4, exit = Quit path (cursor 7, SE 11, return 5 rule) | 15 §1.3 | **CODE BUG** minor (port leaves one call early and resets during the first 4 calls) |
| src/menu.rs:891 (return 5) | runner −1 → `Frame_Run` 2 → WinMain tears everything down and boots again (logo, loading) with the new mode | 00 §2 | **CODE BUG** reachable (port keeps running on the menu) |
| src/menu.rs:921 (0x4C868C on idle frames) | written every call, 32 when no button is down; static, never reset | 15 §1.3 | right (the −1 sentinel is equivalent; the port's reset on menu creation can only add one SE 10 self-swap) |
| src/menu.rs:927 (raw input for idle) | g_InputRawCur only (the polled word) | 15 §1.3 | **CODE BUG** minor (port also counts raw pad buttons; one call early) |
| src/menu.rs:948 (key-config VM layout) | VMs 44 heading, 45–56 rows, 57–74 digit pairs from the working copy, 75/76 ShotSlow; descriptions 0x4C8690 | 15 §1.3, 12 §2.1, §2.2 | **CODE BUG** visual (port prints ASCII rows, no descriptions) |
| src/menu.rs:1057 (gauge limits at menu load) | `MainMenu_OnRegister` → `Ascii_SetupGaugeVms` with the last `Player_Load` limits; hidden in the menu and redone at StageStart | 12 §5.9 (already) | right (unobservable) |
| src/menu.rs:1210 (description VM) | drawn while +0xC2BC ≠ 0; full writer list; option / key-config descriptions exist | 12 §2.2 | **CODE BUG** visual (heuristic; missing option / key-config descriptions; hidden during Quit and Option waits; drawn on the difficulty/replay first frame) |
| src/menu.rs:1386 (> 3 clamp) | difficulty entry > 3 → 1 is the effective value (VERIFIED again) | 12 §2.2, §2.3 (already) | right |
| src/menulist.rs:315 (+0xC290 reset) | zeroed with the menu object (memset at `MainMenu_Register`); the replay entry keeps it | 15 §3.8 | right |
| src/menulist.rs:577 (VM 141) | every path into 11/16 passes a catch-all first; only a still-running slide survives it | 12 §2.1 | right except the < 20-frame round trip (**CODE BUG** minor; the port's VM fault itself is for the implementer) |
| src/menulist.rs:1173 (replay input start) | cursor/paging from call 9, confirm/cancel from call 18 | 12 §2, 15 §3.8 | **CODE BUG** (port: all from call 8) |
| src/menulist.rs:1206 (no recorded stage) | error log line, scene −1 (program exit), return 0; MessageBox at exit | 15 §3.8, 00 §2 | **CODE BUG** (unreachable with game-written replays) |
| src/menulist.rs:1354 (card-name VM) | already in 12 §2.8 step 3 | 12 §2.8 | **CODE BUG** visual (not drawn) |

## New CODE BUGs outside the markers
- Top menu input (12 §2.2 "Input timing", run `play_top`): cursor from call 9, confirm/cancel from call 19; the port
  uses call 10 for both.
- Option item 7 (12 §2 intro, `options_cfg` rows 327 → 357): screen 2 first runs at P + 30; the port (Pending 30) at P + 31.
- Replay paging (15 §3.8): only with > 15 files, wraps; the port clamps for any count.
