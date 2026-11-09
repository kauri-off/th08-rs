# Recorded sessions of the original (small files)

Format: `specs/data/drawlog/README.md` "Sessions". Each directory holds `input.txt` (TH08_INPUT, frame 1 = first main-menu frame), `polls.txt`, `events.txt` (boot seed, loader timing), `meta.txt`, `trace.txt.xz` (36-column state trace), `start/` (score.dat, th08.cfg the game read at launch) and `end/` (files after exit).
Draw logs are large and stay outside git in `sessions/<name>/draw_*.drawlog.xz` (chunks; `cat` them into one .xz stream).

The scripted sessions below are public. `golden` and `fullrun_noclip` are the maintainer's own play: they are local, not in this repository (`tools/private_sessions` finds them; tests skip without them).

- `golden` (maintainer only): human play, 25 634 frames (~7 min): menu → stage 1 → stage 2 → start of stage 3 → back to the menu. Replay in the original is identical (trace and draw log).
- `selftest1`: scripted, 1 922 frames: title → stage 1 → pause → quit → title → quit.
- `gameover_save`, `musicroom_practice`, `options_cfg`: scripted (RE-Q22), see "Scripted reference sessions" below.
- `continue_retry`, `hiscore_entry`, `title_demo`: scripted (RE_SESS_GAPS), see "Scripted sessions for uncovered paths" below.
- `fullrun_noclip` (maintainer only): human play with `--noclip` (`noclip: 1`, 2 733 absorbed hits logged as `noclip_hit` in events.txt), 124 705 frames (~35 min): menu → stages 1–6 → ending/staff roll. Re-verified by a full replay in the original after RE-Q22: 124 708 trace rows, 0 differ, events identical (see `fullrun_noclip/verify.txt`).

## Scripted reference sessions (RE-Q22)

Recorded on Xvfb with `re/tools/xkeys.py` + `record_session.sh` (sound on, music to a null sink), from the work
game copy. All three start from the same `start/score.dat` (= `fullrun_noclip/end/score.dat`, md5 dd5aa0ef…) and
the standard `start/th08.cfg`. Each replays bit-exact in the original (`play_session.sh`: trace 0 differ). The
draw logs (gameover_save 12 MB, musicroom_practice 5 MB, options_cfg 52 KB) stay in `sessions/<name>/`.
Frames below are input frames (frame 1 = first main-menu frame; `meta.txt` menu_base_present gives the present).
"menu" = main-menu screen, "result" = Result state (16 §2).

### `gameover_save` — 12 743 frames, Normal, Reimu & Yukari
| frames | scene | what happens |
|---|---|---|
| 1–780 | 1 menu 0 | title; Start |
| 781–1216 | 1 menu 4 | difficulty (cursor Normal) |
| 1217–1750 | 1 menu 5 | character select, team 0 |
| 1751–1799 | 2 (loading 1751–1799) | stage 1 load |
| 1800–3242 | 2 | stage 1; deaths (pstate 2) at 2564, 2883, 3211 (gm frames 765, 1084, 1412); lives 2 → 1 → 0 → game over |
| 3243–4215 | 2, game over | continue prompt (default "No", 12 §4), declined |
| 4216–5501 | 6 result 0xF | score 31620 ranks > 9 → **no name entry** (16 §2.2), end panel; Shot after 90 frames |
| 5502–5531 | 6 result 0x10 | |
| 5532–6101 | 6 result 10 | "save replay?": Right, Left, confirm "Yes" |
| 6102–6582 | 6 result 12 | slot list: Down, Down, Up → slot No.02, confirm (empty slot → name entry) |
| 6583–7679 | 6 result 13 | replay name entry: cursor starts on END (LSNM chunk present); Down → `P`, type; Left → `O`, type; Bomb = backspace; type `O`; Up → SP cell; Right → END; confirm → saves `replay/th8_02.rpy` name "PO" |
| 7680–7700 | 6 result 2 | fade out |
| 7701–8402 | 1 menu 0 | title (cursor 0); Down ×4 → Replay |
| 8403–9719 | 1 menu 7 | replay list (1 file), choose it, stage 1, speed "normal" |
| 9720–11244 | 2 (loading 9720–9800) | playback of the saved replay: same deaths at 10565, 10884, 11212 (gm 765/1084/1412), ends at gm 1444 |
| 11245–12375 | 1 menu 0 (11245–11279), then 7 | replay ends → title scene; after the menu load the replay screen opens (12 §2.2 "replay was being watched"); Bomb |
| 12376–12743 | 1 menu 0 | Bomb → cursor Quit, Quit |

Files: `replays/th8_02.rpy` (the saved replay; the replay menu reads the copy written during the session). Spec
sections: 11 §4.2, 12 §2.2–2.4, §2.8, §4, 16 §2.1, §2.2, §2.5, §2.6, 15 §3.2–3.8.
Known replay-to-replay differences (by nature, README "Limits"): the date written into the .rpy and shown in the
lists, and the replay's per-second fps samples drawn during playback (15 §3.3) — draw frames 82–142 of the playback
scene differ only in those fps digits. Loading rows of the playback stage load differ in the bullets/enemies columns
(loader-thread timing; `session.py compare` skips them on loading rows).

### `musicroom_practice` — 9 363 frames
| frames | scene | what happens |
|---|---|---|
| 1–419 | 1 menu 0 | title; Down ×6 → Music Room |
| 420–2513 | 8 | music room (16 §5): Down ×2, play No.3; Down ×6 to No.9 (not heard: warning comment); Down ×2 (list scrolls), play No.11; Up ×11 wraps to No.21 (not heard); Bomb → title |
| 2514–3095 | 1 menu 0 | cursor 6 on return; Up ×4 → Spell Practice |
| 3096–3505 | 1 menu 16 | spell practice stage select, Stage1 |
| 3506–4194 | 1 menu 17 | card list: confirm on No.001 (locked → buzzer SE 0x29); Down ×3 → No.004 (灯符「ファイヤフライフェノメノン」 Normal), confirm |
| 4195–4225 | 2 (loading) | |
| 4226–4758 | 2 | attempt 1 (8 lives): card starts gm 92; hit at gm 459 → forced dissolve spell (09 §6.1), card failed (spell 0 at gm 502), game over at gm 533 |
| 4759–5077 | 2, game over | continue prompt, default "Yes" (card not captured); Shot was still held → retry (scene 11) |
| 5078–5669 | 2 (loading 5078–5108) | attempt 2: card gm 92, hit gm 487, card end gm 530, game over gm 561 |
| 5670–7235 | 2, game over | prompt: Down → "No", confirm |
| 7236–7833 | 6 result 10 | spell practice skips name entry (0x16 → 10, 16 §2.1); "save replay?": Right → "No", confirm |
| 7834–7854 | 6 result 2 | |
| 7855–7888 | 1 menu 0 → 15 | top menu forwards (+0x6C) through screen 15 (12 §2.2) |
| 7889–9090 | 1 menu 17 | card list again (No.004 now 0/2 attempts); Bomb |
| 9091–9189 | 1 menu 16 | Bomb |
| 9190–9363 | 1 menu 0 | cursor 2; Bomb → Quit, Quit |

Spec sections: 16 §5, §6.1–6.5, 12 §2.2, §2.6, §4, 11 §4.2, 09 §6.1, 14 (dissolve spell), 16 §2.1/§2.6.

### `options_cfg` — 4 743 frames
| frames | scene | what happens |
|---|---|---|
| 1–343 | 1 menu 0 | title; Up ×2 → Option (wraps from 0 via Quit) |
| 344–1887 | 1 menu 2 | option (12 §2.7, 15 §1.3): Player 3 → 4 (lives 2 → 3); BGM Wav → Off; Vol: Left held 1.2 s (100 → 53); S.E.Vol: Left ×3 (80 → 65); SlowMode Off → On; KeyConfig |
| 1888–4201 | 1 menu 3 | key config (15 §1.3): Right on Shot (no effect: buttons are assigned from the joystick only); Up ×3 → ShotSlow, Right → On; Reset (table 0x4C78C8, ShotSlow = 1); Quit |
| 4202–4547 | 1 menu 2 | back on KeyConfig (cursor 8); Down → Quit, confirm |
| 4548–4743 | 1 menu 0 | cursor 7; Bomb → Quit, Quit (cfg written at exit, 15 §1.2) |

`cfg_diff.txt`: start → end bytes: +0x1C 2 → 3, +0x1F 1 → 0, +0x25 0 → 1, +0x26 0 → 1, +0x27 100 → 53,
+0x28 80 → 65; the pad table is unchanged (Reset restores the same values). A pad-button remap needs a joystick,
which the headless recorder does not have. Slow mode stays on in the end cfg (it is cleared for practice/difficulty ≥ 4
only at stage start, 11 §3.2). Spec sections: 12 §2.7, 15 §1.1–1.3.

## Scripted sessions for uncovered paths (RE_SESS_GAPS)

Same recipe as RE-Q22 (Xvfb, `xkeys.py`, music and SE on to a pulse null sink, work game copy, standard th08.cfg
`6fe6bd78…` with windowed / WAV / SE patched by `record_session.sh`). The three were recorded one after the other
without resetting score.dat, so each `start/score.dat` is the previous session's `end/score.dat`: `title_demo` starts
from `dd5aa0ef…` (= `fullrun_noclip/end`), `continue_retry` from `84fdc6a8…`, `hiscore_entry` from `ab62b80c…` (no
file was crafted). Frames are input frames as above. Verification in the original (`play_session.sh`, draw log on):

| session | trace | draw log |
|---|---|---|
| `title_demo` | 0 differ | 1 221 of 11 278 frames differ: the playback-fps digits (15 §3.3, host time), one stage quad per frame in demo 1 whose vertex colours are uninitialised memory (alpha ≈ 0, alpha-tested away), and a 48×16 sprite at (241.5, 463.5) in the last ~130 frames of demo 0 (both variants also occur in `drawlog/demos/demo0_*`) |
| `continue_retry` | 0 differ | 0 differ |
| `hiscore_entry` | 0 differ | 0 differ |

`session.py compare` skips the loader-timed columns on loading rows; `boss` and `input` are among them (the boss flag
is cleared and the demo's first input word appears on a loading row that moves by ±1 between plays: `title_demo`
rows 9304/9314–9316, `continue_retry` row 8233). `continue_retry` needs the probe build that orders the loader syncs
of `events.txt` by file order (its stage-1 load logged `loader gauge poll 1423` after `release stage poll 1422`).

### `continue_retry` — 13 369 frames, Normal, Reimu & Yukari
| frames | scene | what happens |
|---|---|---|
| 1–1374 | 1 menu 0 / 4 / 5 | Start, Normal, team 0 |
| 1375–1422 | 2 (loading) | stage 1 load |
| 1423–3719 | 2 | idle; deaths at gm 680, 1084, 1412 → game over (gm 1443); continue prompt (default "No"): Up at 3678 |
| 3720 | 2, game over | confirm "Yes" → continue 1 (12 §4 state 3, 11 §4.2): score := continues = 1, lives 2 |
| 3721–6500 | 2 | idle; deaths at gm 1720, 2043, 2346 → game over; Up at 6459, "Yes" at 6501 → continue 2 (score 2) |
| 6502–7134 | 2 | Shot + Left / Right |
| 7135–7556 | 2, paused | Esc → pause (12 §3); Shot on Resume at 7537 → state 4, unpause after 20 frames |
| 7557–7694 | 2 | play; death at gm 2994 (lives 0) |
| 7695–8222 | 2, paused | Esc; Up → Retry (7773); Shot → "no" (7809); Up → "yes" (8166); Shot (8202) → state 10, scene 10 after 20 frames |
| 8223–8271 | 2 (loading) | retry = fresh start (gm 0, score 0, lives 2, 11 §3.2) |
| 8272–9346 | 2 | play; Esc at 8852, Down → Quit, Shot, Up → "yes", Shot (9326) → state 9 → title |
| 9347–11022 | 1 menu 0 / 8 / 9 / 11 | Down ×3 → Practice Start; Normal, team 0, Stage1 |
| 11023–11079 | 2 (loading) | stage practice load |
| 11080–11523 | 2 | play; Esc at 11299, Up → Retry, Shot, Up → "yes", Shot (11503) → state 10 → scene 11 |
| 11524–11554 | 2 (loading) | practice reload (scene 11) |
| 11555–12292 | 2 | play; Esc at 12068, Down → Quit, Shot, Up → "yes", Shot (12272) |
| 12293–12719 | 1 menu 0 → 8 → 9 → 11 | top menu forwards back to the stage select (12 §2.2) |
| 12720–13369 | 1 menu 9 / 8 / 0 | Bomb ×3 to the title; Bomb → Quit, Shot |

Spec sections: 12 §3, §4, 11 §4, §4.2, §3.2, 12 §2.2–2.5.

### `hiscore_entry` — 18 261 frames, Normal, Reimu & Yukari
`start/replay/th8_01.rpy` = `gameover_save/replays/th8_02.rpy` (name "PO"), so slot No.01 is occupied. The
Normal / team-0 ranking in `start/score.dat` has one HSCR entry (34292650) and DMYS defaults below it (15 §2.3), so
any score ≥ 200 000 ranks (the 11th node, the 100 000 default, is dropped).

| frames | scene | what happens |
|---|---|---|
| 1–986 | 1 menu 0 / 4 / 5, 2 (loading 934–986) | Start, Normal, team 0 |
| 987–4967 | 2 | Shot held, Left / Right zigzag; deaths at gm 1065, 1958, 2293 → game over at gm 2325 (score 306 620); continue prompt, Shot released |
| 4968 | 2, game over | confirm "No" |
| 4989–8270 | 6 result 9 | hi-score name entry (16 §2.2), rank 9 (row "9"), cursor starts on END: Down → `P`, Shot; Left ×3 → `M`, Shot; Bomb = backspace; Right → `N`, Shot; Up / Down / Up / Right / Down / Right → END; Shot at 8271 → name "PN" |
| 8271–8657 | 6 result 0xF / 0x10 | end panel; Shot at 8628 |
| 8658–9071 | 6 result 10 | "save replay?" "Yes" |
| 9072–9523 | 6 result 12 | slot list, cursor on No.01 ("PO", occupied), Shot |
| 9524–10019 | 6 result 14 | "overwrite?" (16 §2.6) "Yes" |
| 10020–10462 | 6 result 13 | replay name entry, cursor on END, name "PN" from LSNM; Shot → saves `replay/th8_01.rpy` |
| 10463–10483 | 6 result 2 | fade out |
| 10484–11098 | 1 menu 0 | Down ×5 → Result |
| 11099–11691 | 5 result 1 | Player Data top menu (16 §2.4) |
| 11692–14465 | 5 result 3 / 4 / 5 | ranking: difficulty list (Right ×2 no effect, Down → Hard, Up → Normal), team list, Normal / team 0 table (row 9 "PN"); Right → team 1 table; Down; Bomb ×3 back |
| 14466–16770 | 5 result 6 / 7 / 8 | spell cards: "all difficulties", "all characters", page 1; Right → page 2 (No.11–20); Down → team 1; Bomb ×3 back |
| 16771–17520 | 5 result 1 / 19 / 20 / 21 | Down → other stats (play counts, times, clears, continues, practice); Bomb |
| 17521–17643 | 5 result 1 / 2 | Down → back to title, Shot |
| 17644–18261 | 1 menu 0 | Bomb → Quit, Shot |

Files: `replays/th8_01.rpy` (the overwritten slot), `end/score.dat` (new HSCR "PN" 306620, LSNM "PN"). Spec sections:
16 §2.1–2.6, 15 §2.3, 12 §2.2, 12 §4.

### `title_demo` — 11 260 frames
| frames | scene | what happens |
|---|---|---|
| 1–1550 | 1 menu 0 | title, no input; idle counter reaches 1500 (12 §2.2 "Demo") → `demorpy0.rpy` |
| 1551–1631 | 2 (loading), demo 0 | demo load |
| 1632–7751 | 2, demo 0 | attract demo 0 runs to its limit (fade at 6000, scene 1 at 6120, 11 §4 step 6) |
| 7752–9301 | 1 menu 0 | title again, no input → `demorpy1.rpy` |
| 9302–9382 | 2 (loading), demo 1 | |
| 9383–10496 | 2, demo 1 | demo 1; Shot at 10496 (demo frame 1114) → scene 1 |
| 10497–11260 | 1 menu 0 | title; Bomb → Quit, Shot |

Spec sections: 12 §2.2 (demo counter), 11 §4 step 6, 11 §2 (demo index), 15 §3.3 (playback fps text).
