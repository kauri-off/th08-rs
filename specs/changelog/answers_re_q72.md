# RE-Q72 — top-menu idle counter and attract-demo start (2026-11-07)

Question: docs/briefs/IMPL151_QUESTIONS.md "SESSFIX" 1 (12 §2.2 "Demo": increment before or after the test, `>=` or
`>`, what resets it, does the demo call skip the rest).

## Changed spec sections
- 12 §2.2 "Demo" — exact order, compare, constant, resets, success / failure writes, return 0.
- 12 §2 "Handlers that return 0" — the attract-demo start added to the list.

## Evidence (static, `MainMenu_Top` 0x4674E0)
+0x14430 is referenced only in this function (all_decomp: 0x467959, 0x467A9C–0x467AA6, 0x467ABA, 0x467AC4,
0x467B59). Disassembly:
```
0x467959 and dword [eax+0x14430], 0        ; call 9 (sub-state 0 → 1), falls into LAB_00467960
0x467960 push 9 / call Menu_CursorUpDown   ; sub-state-1 entry (dispatch jumps here when +0x0C == 1)
...                                        ; cursor wrap over locked items, highlight
0x467a9c mov eax,[eax+0x14430]; inc eax; mov [ecx+0x14430],eax
0x467aac movzx eax, word [0x164d528]       ; g_InputRawCur
0x467ab3 test eax,eax; je 0x467ac1
0x467aba and dword [eax+0x14430], 0
0x467ac4 cmp dword [eax+0x14430], 0x5dc ; jle 0x467c5b   ; demo only when counter > 1500
0x467ad4 0x164d0bc := (0x164d0bc+1) % 4 ; sprintf ; FileSystem_OpenFile ; Replay_Load
0x467b50 call 0x437cdd "error : Demo Play is not ready" ; 0x467b59 counter := 0 ; jmp 0x467c5b (change test)
0x467b65.. success writes ; 0x467c34 scene 0x17ce8b4 := 2 ; 0x467c3e 0x164d0bd := 0 ; flags &= ~0x4000 ;
0x467c56 jmp 0x468349 with return value 0
```
0x467C5B is the description change test; the end-of-call increments (+0xC294, +0x10, +0x1442C) are at 0x468313, so
the successful demo call skips them.

## Cross-check with sessions
- `title_demo`: the menu loader releases at poll 40 (events.txt), top-menu call 0 = poll 41, call 9 = poll 50
  (counter 1), call 1509 = poll 1550 sets the demo flags, scene 2 from poll 1551 (trace). Second title: release at
  poll 7791, demo call 7791 + 1 + 1509 = poll 9301, scene 2 at 9302 (trace). `>= 1500` would give polls 1549 / 9300,
  i.e. the port's one-call-early start.
- Longest input-free top-menu stretch in the other sessions (trace, scene 1 menu 0 rawinput 0000): selftest1 343,
  gameover_save 779, continue_retry 744, hiscore_entry 639, musicroom_practice 478, options_cfg 253 — all < 1501, so
  none may start a demo (consistent, not discriminating).
