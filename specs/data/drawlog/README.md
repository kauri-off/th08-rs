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
