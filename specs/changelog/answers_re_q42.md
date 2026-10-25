# RE-Q42 — implementer questions: menus, key config, restart, Info loop (IMPL105 Q1–5, IMPL106 Q1–3) (2026-10-25)

Brief: docs/briefs/RE_Q42_BRIEF.md; questions docs/briefs/IMPL105_QUESTIONS.md, IMPL106_QUESTIONS.md. Run notes:
re/work/runs/q42/CHECKPOINT.md. Evidence: decompile + disassembly of `MainMenu_Top` 0x4674E0, `MainMenu_Option`
0x46837B, `Menu_SetScreen` 0x4694C3, `MainMenu_KeyConfig` 0x469636, `KeyConfig_SetDigits` 0x469FA9, `MainMenu_Replay`
0x46E136, `MainMenu_LoaderThread` 0x470E10, `MainMenu_OnRegister` 0x470A6C, `Info_Show` 0x471181, `Input_Poll`
0x43D970, `Supervisor_Calc` 0x445453, `Supervisor_Register` 0x445EA7, `Supervisor_OnRegister` 0x445FA3,
`Supervisor_Init` 0x4464C8 (0x4464F9 `mov ax, [0x17CE900]`), `AsciiManager_Register` 0x402780 / `AsciiManager_Load`
0x402830, `AnmFile_StartScript` 0x45E580, `AnmVm_Init` 0x4068E0, `Text_RenderCentered` 0x466650, WinMain 0x4418C0
(0x441A70 `mov byte [0x17CE710], 0xFC`); anm dump (title01 scripts 1–9, text.anm script 9). Check only (not derivation):
`options_cfg` trace + draw log presents 300–366 (Option press at 327: items static and item 7 lit through 356, slide
from 357). No new run.

## Changed spec sections
| spec § | change |
|---|---|
| 12 §2 intro | "a press zeroes +0x10" qualified; replay screen: +0x10 > 9 gate armed once per visit, no gate in sub-states 2/3 |
| 12 §2.2 | Highlight rule (entry call 0, cursor moved, cancel only); item 7 row (no range interrupt, no highlight, cancel fall-through, sub-state 3 contents, inline switch); Input timing wording; description VM loader set-up + `Text_RenderCentered` visible bit |
| 15 §1.3 | key-config / option cursor-0 writers; key-config per-call order (digits before assign/toggle/reset; runs in the exit call; no input gate) |
| 15 §3.8 | replay gate pointer |
| 16 §4 | `Info_Show` poll = full `Input_Poll` (joystick + ShotSlow counter), which words it does / does not write |
| 00 §2 | what the restart re-initialises (Supervisor fields, skip counter −4, score.dat flush + reload, RNG, ASCII object, DI); skip counter start −4 at every start |

## Answers
| question | original behaviour (evidence) | spec § | port |
|---|---|---|---|
| IMPL105 Q1 highlight every call? | No. Sprite/colour highlight runs in the entry block (call 0), in sub-state 1 only when `Menu_CursorUpDown` ≠ 0, and on cancel. Not in calls 1–8, sub-states 2/3, nor on confirm. Option confirm sets cursor 0 without it → item 7 stays lit through the wait and the slide-out (draw log 328–366). menu_clr's unselected sprites from the first frame = the call-0 entry block. | 12 §2.2 "Highlight" | CODE BUG (every call → item 0 lit during Option's wait / slide) |
| IMPL105 Q2 key-config cursor 0 | Option item 8 confirm: cursor := 0, `Menu_SetScreen(3)`; key-config entry does not write it. Option cursor 0: top item-7 confirm (twice) and the inline switch in P + 29. | 15 §1.3 | right (equivalent: nothing reads the cursor in between) |
| IMPL105 Q3 restart resets | Register: +0x158 := 0, +0x15C := −1, +0x154 := 0 (+0x160 := 0 later at boot → menu). WinMain: Result(2) flush → score.dat save before teardown; skip counter byte := −4, timers 0; OnRegister: game speed 1, score.dat reload, flags 15–17; Init: g_Rng u16 := timeGetTime (counter kept), ASCII object zeroed by `AsciiManager_Load`, DI recreated, keyboard VK state bit 7 cleared. Input words, 0x164D53C, 0x164D538, 0x164D51C untouched. | 00 §2 | scene right; CODE BUG: skip counter not −4 (also at first boot), no score.dat flush on the restart path; g_Rng not reseeded (check) |
| IMPL105 Q4 replay +0x10 gate | Gate only in sub-state 1; +0x10 zeroed only by the list cancel (→ 4). File confirm keeps +0x10; cancels 2→1 / 3→2 zero +0x1442C only → gate open on return. Sub-states 2/3: no gate. | 12 §2 intro, 15 §3.8 | CODE BUG (port zeroes `rpy_t` on confirm and on 2→1, re-gating 10 calls) |
| IMPL105 Q5 Option wait | Sub-state 3 runs only the +0x10 > 29 test and the counter increments. The press call: cursor 0, sub-state 3, in-place `MainMenu_Option` (counters only), cursor 0, then falls through to the cancel test (Bomb/ESC edge → cursor 8 + SE 11), increments. No range interrupt at the press. | 12 §2.2 item 7 | CODE BUG (port `interrupt_all(5)` at the press: items slide out 30 frames early; cancel fall-through missing) |
| IMPL106 Q1 0x469FA9 position | Digits + ShotSlow VMs (steps 4–5) run before joystick assign, ShotSlow toggle, Reset and the exit tests → a change shows one call later; they also run in the Quit / idle-exit call. | 15 §1.3 "Call order" | CODE BUG (port: after Reset, not in the Quit call) |
| IMPL106 Q2 Info_Show poll | Same `Input_Poll`: keyboard + `Input_PollJoystick` (pad bits, ShotSlow counter 0x164D51C stepped per iteration). Writes only g_InputRawCur; 0x164D530, 0x164D53C, 0x164D538, game words untouched. | 16 §4 step 3 | right |
| IMPL106 Q3 description VM sprites | Loader: `AnmFile_StartScript(text.anm, script 9)` (one run → sprite 21), then `SetSprite(sprite + i)` = 21 + i. Outside script 9 only +0x1FE (handlers) and `Text_RenderCentered` (sets flag bit 0 visible; reads +0x298/+0x299, 0 → 15). No pos / colour writes. | 12 §2.2 "Loader set-up" | right (the visible-bit re-set draws nothing: alpha 0) |

## CODE BUG list
1. Top menu highlight every call (IMPL105 Q1) → only call 0 / cursor moved / cancel; Option keeps item 7 lit.
2. Top item 7 sends range interrupt 5 at the press (IMPL105 Q5) → none; items slide on the option entry's interrupt 3
   (P + 30). Also add the cancel test in the press call.
3. Replay list re-gates confirm/cancel after a file confirm / return from sub-state 2 (IMPL105 Q4).
4. Key-config digits / ShotSlow plates updated after the assign / toggle / Reset in the same call (IMPL106 Q1) →
   before them (one-call lag), and also in the exit call.
5. Restart / boot (IMPL105 Q3): Frame_Run skip counter starts at −4 (first draw in frame 4); the restart path runs the
   Result mode-2 score.dat flush before teardown and reloads score.dat; g_Rng reseeded from the time.
