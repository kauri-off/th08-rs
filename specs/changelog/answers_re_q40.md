# RE-Q40 — SPEC-GAP batch 6: replay, score.dat, supervisor, config (2026-10-24)

Brief: docs/briefs/RE_Q40_BRIEF.md; markers: docs/specgaps/batch6_replay_scoredat_supervisor.txt (14). Runs:
re/work/runs/q40/ (CHECKPOINT.md). Evidence: decompile / disasm of `ExeCheck` 0x44858D, the version-list loader
0x4461A0, `FileSystem_OpenFile` 0x43E660, `ScoreDat_Load` 0x45A5E0 (chunk walk), `ScoreDat_Save` 0x453D0D,
`Supervisor_Calc` 0x445453 (top + 0x445AFF), `Supervisor_StartLoader` 0x4486BA, `Supervisor_Init` tail, `MainMenu_Calc`
0x467399 / `MainMenu_OnRegister` / `MainMenu_LoaderThread`, `Fps_Update` 0x446F53 and its callers (0x445BC0 draw prio
16, `GameManager_StageStart`), `Config_Load` 0x4477B1, 0x437CDD (empty), `Replay_Finish` 0x4531F0, `Replay_RecordCalc`
0x452310, `Replay_PlayCalc` 0x452550, `Replay_FastForward` 0x452490; strings / pointer tables 0x4C7F30, 0x4C7F4C,
0x4C7FAC, 0x4C7FD4, 0x4B6D08, 0x4B660C, 0x4B668C, 0x4B6C74 (rd.py); data: th08_0100d.ver from th08.dat, the demo
headers (tags / exe values), the fps streams of the original-recorded replays (re/work/runs/q22/th8_02_gameover.rpy:
sample 0 = 0, 49 × 60). structxref writers of 0x17CE8B4. No new run.

## Changed spec sections
| spec § | change |
|---|---|
| 15 §1.2 | log texts (missing vs invalid); thbgm.dat-missing message goes to the empty 0x437CDD (nothing shown); bad header fails in both paths |
| 15 §2.3 | saver rewrites +0x14/+0x15/+4/+6/+8 and clears +9 in place before copying |
| 15 §2.10 | rewritten: th08_0100d.ver is an archive member (missing → start-up error), full list, ExeCheck algorithm (debug tag, line walk, `%d` wrap, over-run quirk), the three callers, port guidance; VRSM +9/+0xA |
| 15 §3.2 | slow byte = gameplay copy `*(GM+4)`; fps_now writers, carried value in sample 0, 60 at steady 60 fps |
| 15 §3.3 | slow byte source; Fps_Update meter (draw prio 16 + StageStart call, 1 + frame-skip count, window never reset → phase from boot); FastForward: one frame counter, dialogue test first, both gated |
| 15 §3.5 | USER キャラ名 (0x4C7F4C, padding kept), 難易度 (0x4C7F30), 最終ステージ (bit 4 / 0x453CD1 / 0x4C7FAC), card line |
| 15 §3.6 | card name source CATK[id] +0x10 in memory |
| 11 §7 | Supervisor_Calc early exits (boot loader wait returns 1); 6 → −1 unreachable; +0x154 = ExeCheck every 4000 frames (not a config check) |
| 12 §1 | what ends the boot and menu-load holds (thread completion, MainMenu+0x14434), host-timed |
| 16 §2.3 | +9 cleared by every save |

## Markers
| marker | original | spec | port |
|---|---|---|---|
| scoredat.rs:357 VRSM check | every VRSM v1 chunk: `ExeCheck(tag, size, sum)` against th08_0100d.ver (archive); fail → file rejected | 15 §2.10, §2.1 | CODE BUG (not made; reachable only with a file whose tag/size/sum is unlisted) |
| scoredat.rs:590 HSCR +9 | `ScoreDat_Save` sets +9 := 0 on the in-memory entry, then copies | 15 §2.3 | right |
| supervisor.rs:36 holds | boot: Supervisor_Calc returns 1 until the `Supervisor_Init` thread ends; menu: MainMenu_Calc returns 1 until `MainMenu_LoaderThread` sets +0x14434 := 0 (+ Loading_End); host thread time | 12 §1, 11 §7 | right (any fixed count is allowed; the measured counts are fine) |
| supervisor.rs:112 fps text | refresh when the Fps_Update meter closes a ≥ 500 ms window; window continuous since boot, never reset | 15 §3.3 | right (approximation; the spec's 60 Hz model counts drawn frames from boot, the port's per-gameplay phase is equally unreproducible) |
| supervisor.rs:270 6 → −1 | Replay_Finish(0) then quit, but no −1 request can happen in scene 6 | 11 §7 | right (unreachable) |
| supervisor.rs:377 +0x154 | every 4000th calc: `ExeCheck` of the running exe → fail = return 4; passes for 1.00d | 11 §7, 15 §2.10 | right (no-op with the 1.00d constants) |
| config.rs:166 message box | 0x437CDD is an empty function: nothing shown | 15 §1.2 | right (a log line is not observable) |
| replay.rs:172 exe check | `ExeCheck(info+0x12C, +0x124, +0x128)`; demos pass (0100a listed / "debug") | 15 §2.10, §3.7 | CODE BUG (not made) |
| replay.rs:571 card name | 48 bytes from 0x160F558 + id·0x22C = in-memory CATK[id] +0x10 | 15 §3.6 | right |
| replay.rs:580 USER tables | キャラ名 0x4C7F4C full string incl. U+3000 padding; 難易度 0x4C7F30[diff]; stage 0x4C7FAC via 0x453CD1 | 15 §3.5 | tables right; CODE BUG: キャラ名 padding trimmed (`trim_end_matches('　')`) |
| replay.rs:702 record slow | `*(GM+4)` +0x25 (gameplay copy) | 15 §3.2 | right |
| replay.rs:721 fps_now | sample k ≥ 1: measured (60 at 60 fps); sample 0: carried u8 (0 after boot, last playback byte after a demo) | 15 §3.2 | CODE BUG (minor, file bytes only: sample 0 written as 60) |
| replay.rs:742 playback slow | `*(GM+4)` +0x25 | 15 §3.3 | right |
| replay.rs:799 FF order | gate bit 2 & !bit 9; dialogue (`+0 % 3 ≠ 2`) first, else fast (`+0 % 5 ≠ 4`, no boss) | 15 §3.3 | CODE BUG (fast first, own dialogue counter `ff`, no bit 2 / bit 9 gate) |

## CODE BUG list
1. score.dat load: VRSM exe check missing (15 §2.10): reject the file when a VRSM v1 chunk fails `ExeCheck` against the
   th08_0100d.ver list read from th08.dat.
2. Replay load: the same check on info +0x12C/+0x124/+0x128 (15 §3.7 step 5).
3. Replay USER キャラ名: print the 0x4C7F4C name with its U+3000 padding (15 §3.5).
4. Replay record fps sample 0 = the carried fps_now (0 after boot / last playback byte / 60 after a played game), not
   always 60 (15 §3.2).
5. Replay_FastForward: test only while bit 2 set and bit 9 clear; dialogue rule first with the replay frame counter
   (`frame % 3 ≠ 2`), fast rule only otherwise (15 §3.3).
