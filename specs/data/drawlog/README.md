# Reference draw logs from the original (autocmp)

These logs come from the original th08.exe, recorded with the probe (`re/probe/`, regeneration in
`re/TOOLING_REPORT.md` §6). The format is described in `FORMAT.md`. The tool is `re/tools/drawlog.py`
(`summary`, `render`, `diff`, `frames`, `split`).

## Contents

| file | content |
|---|---|
| `demos/demo<K>_<a>-<b>.drawlog.xz` | attract demo K (= `demorpy<K>.rpy`), every presented frame from the first loading frame of the demo to its end, split by demo frame (1500 per chunk; the first chunk also holds the loading frames, `demo_frame` 0). `demo_frame` matches column 2 of `specs/data/demo<K>_trace.txt`. |
| `demos/demo<K>.summary.txt` | `drawlog.py summary` of all chunks of demo K |
| `demos/dl_d<K>_f<F>.png` | screenshot of demo K at game frame F (0x164D2C8). The draw log frame with the same `present` (`F` line field 1) is that image. d0 f1400 = present 3043, d0 f2200 = 3843, d1 f1000 = 10387, d2 f1000 = 16930, d3 f1000 = 23455, d3 f4000 = 26455 |
| `menu_base.drawlog.xz` | the menu script (below), boot → … → back to title, every frame from Present 1 |
| `menu_base.input.txt` | its input, `TH08_INPUT` syntax (`frames:mask`, comma separated) |
| `menu_base.shots.txt` | `<script frame> <name>`: frames at which `menu_ref/<name>.png` was taken |
| `menu_clr.*` | the same with Final A/B clear flags set (character-select part only) |
| `menu_ref/*.png` | 640×480 screenshots of both menu scripts |
| `th08.cfg` | the config used for all runs (60 bytes; +0x21 = 1 Normal, +0x1F music 0, +0x20 SE 0, +0x22 window 1) |

Each demo log covers the whole demo with no gaps: 6120 / 4920 / 5040 / 7020 demo frames plus loading.

## Menu scripts

**Start state.** No `score.dat` (fresh: Extra Start and Spell Practice locked, 4 teams). `th08.cfg` is the
installed one with window mode on and music/SE off. Its last difficulty (+0x21) is **Normal**, and this is
what the difficulty cursor starts on.

**Frame numbering.** Script frame 1 = the **first presented frame of the main menu** (scene 1, `scene_frame` 1).
Script frame k gives the input word the game uses in that frame. The boot/logo/loading part before it lasted
13–14 presents in the original. It varies by a frame between runs because the loader thread races the main
loop. To replay in the port, prepend `<port boot frames>:0` (the frames before the port's first menu frame) to
`TH08_INPUT`, or start the script when the menu scene starts.
When comparing draw logs, match frames by `(scene, scene_frame)`: `drawlog.py diff --key scene`. The demo logs
use `--key demo`.

**Keys.** Mask bits are the game's input word (12 §0): 0x1 shot/confirm (Z), 0x2 bomb/cancel (X), 0x10 up,
0x20 down, 0x40 left, 0x80 right. The only presses used are 3-frame presses of Z, X, down and right, at least 30
frames apart, so the key-repeat logic never starts.

**menu_base** (2510 frames). The shot names in `menu_base.shots.txt` say where each screenshot is taken. Each
shot is taken 5 frames before the next press, so the screen has settled.
1. f300 Z: Start → difficulty select. Down ×4, 30 frames apart: N → H → L → E → N.
2. Z → character select, team 0. Right ×4, 70 frames apart: teams 1, 2, 3, 0.
3. X back to difficulty, X back to the top menu.
4. Down → Practice Start (Extra Start and Spell Practice are locked and skipped). Z (difficulty), Z (character),
   Z (practice stage select). Then X ×3 back to the top menu.
5. Down → Replay, Z (replay list, empty), X back.
6. Down → Result, Z (scene 5, result viewer), X, X (the first X moves the viewer cursor to 「タイトルに戻る」,
   the second leaves) → top menu with the cursor on Result.
7. Down → Music Room, Z (scene 8), X back.
8. Down → Option, Z (option screen), down ×3, X (cursor to Quit), Z → top menu.

**menu_clr** (970 frames). Step 1–3 of menu_base. At script frames 100 and 280 the CLRD clear masks in memory
(GM+0x3C490 = 0x164B998 + 0x24·team, 15 §2.4) are set for all four difficulties E–L:

| team | any-clear (+0x16) | no-continue (+0x0C) | notice shown (12 §2.4) |
|---|---|---|---|
| 0 Reimu & Yukari | 0x41 (6A) | 0x01 | sprite 145 「——FinalB 挑戦可能——」 |
| 1 Marisa & Alice | 0x81 (6B) | 0x81 | 148 「——FinalB クリア——」 |
| 2 Sakuya & Remilia | 0xC1 | 0xC1 | 146 「——FinalA,B クリア——」 |
| 3 Youmu & Yuyuko | default 0x01 | 0x01 | none |

To reproduce this in the port, create a score.dat with those CLRD values, or set them after loading. The
screenshots `menu_ref/clr_m_char_team0..3.png` show the notices.

Observed timing fact: the notice follows the cursor **in the frame of the key press**, while the portraits
still animate. A screenshot taken on the press frame shows the next team's notice over the previous team's
portraits.

## Run-to-run determinism (measured: menu_base vs menu_clr, same input up to the CLRD pokes)

- The **fps counter** ("60.00fps", ascii.anm, bottom right) shows measured real time and differs between runs.
  `drawlog.py diff` always drops primitives from that corner.
- The **title intro**, the first ~105 frames of the first main-menu visit (the white fade-in and the item
  slide-in), can shift by one frame between runs. Its start depends on the loader thread.
- After that, the two original runs are identical frame for frame until the intended difference (the
  Final-route notice from script frame 489 on). The only other differences are 2–3 frames of the top-menu item
  wobble. So a correct port should match every frame keyed by `(scene, visit, scene_frame)`, except those
  intro frames.
- Demos are deterministic. Three earlier runs gave identical traces (answers_phase8). The draw logs do not
  depend on wall time, except the fps counter.

## Size

The demo logs are about 91 MB of xz in total (≈ 57 KB of text per gameplay frame before compression, mostly bullets).
If that is too much for the repository, keep the first chunk of each demo and regenerate the rest
(`re/TOOLING_REPORT.md` §6, ~10 min).

## menu_mopup1 (MOPUP-1, list screens)

Start state differs from menu_base: `menu_mopup1.score.dat` (team Reimu & Yukari has records, Spell Practice
unlocked) and the four attract-demo replays copied to `replay/th8_01.rpy` … `th8_04.rpy`. Same th08.cfg.
Script `menu_mopup1.seq.py` (presses + shot names, 3-frame presses), input `menu_mopup1.input.txt`, shots
`menu_mopup1.shots.txt`; draw log of all 2962 presents. Covers: spell practice screens 16/17 (incl. info-panel
refresh, page change), practice stage select, replay list / detail / speed choice, result viewer top menu,
D-key dump (`menu_mopup1.score.txt` is the score.txt it wrote), difficulty/character/ranking, card group/
character/list (page change and character change with the accordion), play data. Twelve screenshots are in
`menu_ref/mopup1_*.png`.

## Sessions (recorded human play, `sessions/<name>/`)

Recorded with `re/tools/record_session.sh` (user guide: `sessions/README.md`). A session covers one run of the original
from process start to exit, menus included. VERIFIED: replaying a session in the original (`re/tools/play_session.sh`)
gives an identical trace and an identical draw log (two scripted sessions: 1 922 rows and 10 654 rows incl. ~2.7 min
of stage 1 with movement, focus, bombs, pause/resume, quit; 0 differences, apart from the fps counter, which
`drawlog.py diff` ignores).

### Input: `input.txt` and `polls.txt`
- The input word is what the game's input function `Input_Poll` (0x43D970) **returned**: keyboard (DirectInput or
  GetKeyboardState) and joystick combined, bits as in "Keys" above. The game calls it once per frame at the start of
  `Supervisor_Calc` (calc prio 0) and stores it in `g_InputRawCur` 0x164D528. The previous word and the menu
  key-repeat counter are derived from it (11 §7), and the gameplay input `g_Input` 0x164D52C is taken from it after
  the objects ran (replay recorder). Feeding these words as the port's raw input reproduces everything downstream.
  Exception: with slow mode (cfg +0x25) on, a stalled GM frame sets 0x160F535, and the next frame ORs the new word
  into the old one instead of replacing it. That is game logic; the file holds the function's return values.
- `input.txt`: `TH08_INPUT` syntax (`count:mask`, comma separated, `#` comments). **Frame 1 = the first presented
  frame of the main menu** (scene 1, scene_frame 1), the same convention as `menu_*.input.txt`. Frame k is the word
  polled during the calc of the frame presented at `menu_base_present + k − 1` (`meta.txt`). Boot frames before it
  never poll input. Header comments count frames without a poll (the previous word is repeated) and presents with
  more than one poll (the last one is written); both were 0 in all test sessions. During normal play there is
  exactly one poll per presented frame. The blocking unlock notice (`Info_Show` 0x471181) polls once per present
  outside Frame_Run, marked `I` in polls.txt.
- `polls.txt`: `poll present scene src mask` per call, exact. `present` = number of the presented frame the poll
  belongs to (draw-log `F` field 1), `scene` = scene id at the moment of the poll (before this frame's scene switch),
  `src` = `S` (Supervisor_Calc) or `I` (Info_Show), `mask` = hex word.

### State: `events.txt`, `start/`
- `seed <hex32> present P poll N`: the boot seed = `timeGetTime()` stored at 0x17CE900; its low 16 bits are the RNG
  state (`Supervisor_Init` 0x4464FF, 11 §1). It is the only non-deterministic RNG input; RNG writes in
  GameMgr_Calc/StageStart are save/restore pairs, and `setstate` lines (replay playback, `Rng_SetState` 0x453BE0)
  come from .rpy contents.
- `loader <kind> poll B present P` / `release <kind> poll P` / `release boot calc C`: loader-thread timing (below).
- `joyraw <n> <32 hex bytes>`: n-th call of `Input_PollJoystickRaw` (key-config capture) when a button was down.
- `start/th08.cfg`, `start/score.dat`, `start/replay/*.rpy`: the files the game read at launch. To replay in the port,
  start from exactly these files.

### Loader threads and frame alignment (VERIFIED by replay)
Three loaders run on a second thread, concurrently with the main loop (`Supervisor_StartLoader` 0x4486BA): boot
(`Supervisor_Init`), main menu (`MainMenu_LoaderThread`) and every stage (`GameManager_StageStart`). How many main-loop
frames they take varies from run to run (they wait for the main thread's texture post-load, one entry per frame).
This race is the reason two plain runs of the original differ (title intro ±1 frame, gauge VM age, first gameplay
frame). A session pins it:
- `release boot calc C`: the boot loader finished before the C-th Frame_Run calc (the first frame that polls).
- `release menu poll P`: the menu loader's tail (title music, white fade-in `ScreenEffect_Register(0, …)`,
  MainMenu+0x14434 = 0, `Loading_End`) ran before the calc of the frame whose poll index is P.
- `loader gauge poll B`, `loader lwait poll B`, `release stage poll P`: the stage loader set up the gauge VMs
  (`Ascii_InitVms`/`Ascii_SetupGaugeVms`), called `Loading_Wait` (needs GM+0x3C ≥ 30 loading frames) and cleared
  GM+0x38 (0x160F540) before the calc of the frame with poll index B / B / P. GameMgr_Calc runs the first gameplay
  frame in frame P.
A port that loads synchronously should reproduce these frame counts, not its own loading time: e.g. stay on the
loading screen until poll P, then run gameplay frame 1. Stage loading rows of `trace.txt` (loading column ≠ 0) show
the loader's progress (player position, rank, flags bit 2) at timing-dependent points. Compare only from the first
row with loading = 0.

### Trace: `trace.txt.xz`
One row per calc pass from boot to exit, plus a row (`ff` = −1) for presents without a Frame_Run calc (boot,
`Info_Show`). Columns 1–23 are those of `specs/data/demo*_trace.txt` (frame demo_frame input rng rng_ctr px py
pstate score graze points lives bombs power gauge time bullets enemies rank spell boss flags ff), with
`rng_ctr` = all `Rng_Next16` calls since process start (main and loader thread), and the gameplay stats are zero
outside scene 2 and while loading. Then:

| col | name | meaning |
|---|---|---|
| 24 | present | presented-frame number of this frame (draw-log `F` field 1) |
| 25 | calc | Frame_Run calc count |
| 26 | poll | polls so far (= poll index of this frame + 1) |
| 27 | scene | Supervisor scene after this calc |
| 28 | scene_frame | as draw-log `F` field 3 |
| 29 | menu | MainMenu screen (+0x14428) at the start of MainMenu_Calc this frame, −1 if it did not run |
| 30 | cursor | MainMenu +0x00 at the same point, −1 if not run |
| 31 | result | Result object state (+0x08) after Result_Calc this frame, −1 if not run |
| 32 | pause | pause byte 0x164D0BA |
| 33 | rawinput | `g_InputRawCur` 0x164D528 after this calc |
| 34 | stage | g_StageIdx |
| 35 | loading | 0x160F540 (GM+0x38) |
| 36 | demo | attract demo index, −1 if none |

Compare a port trace with `re/tools/session.py compare <rec dir> <port dir>` (rows aligned at the first main-menu row).

### Draw log: `draw_NNNNN.drawlog.xz`
Same format as FORMAT.md, every presented frame from present 1 to exit, split into chunks of 1 800 presents
(`NNNNN` = chunk number). Read the chunks in order or pass the glob to the tools:
`drawlog.py diff "sessions/<name>/draw_*.drawlog.xz" "<port>/draw_*.drawlog.xz" --key scene`.
Non-finite numbers are written `nan` / `inf` (older logs: msvcrt spelling `-nan(ind)`; `drawlog.py` reads both).
Observed once: a `stg1bg.anm` quad (depth-tested, rhw 1) with NaN x/y/z at stage 1 gameplay frame 4942 (session
`selftest2`, present 7538). The original submits it and D3D draws nothing for it (INFERRED).

### Tooling fix found while building this (2026-10-18)
`drawlog.py diff` handed every primitive the frame's **last** T/S/V state: `prims()` yielded one shared dict, and
the diff collected the primitives into a list. Texture/state differences were therefore not detected, and the fps
filter could miss. Fixed (each primitive gets a copy). The run-to-run determinism notes above were measured before
the fix. Re-measured: `menu_base` vs `menu_clr` differ in 381 of 982 compared frames, from menu frame 33 on. That is
the menu-loader race described above, not only the intended difference from script frame 489.
