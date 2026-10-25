# RE-Q47 — IMPL113 questions 1–3 (replay entry block, save-dialog flush, restart PLST test)

Code only (decompile + capstone disasm of th08.exe, re/tools/dispscan.py). No runs of the original.

| # | question | original | spec | port |
|---|---|---|---|---|
| 1 | replay screen: do the cancels 2 → 1 / 3 → 2 (which zero +0x1442C) re-run the entry block? Anything else read +0x1442C in sub-states 1–3? | `MainMenu_Replay` 0x46E136 dispatches on +0x0C first (0x46E156 `je 0x46E198`); the +0x1442C == 0 test (0x46E19E) and the `> 7` test (0x46E42E) are both inside the sub-state-0 case. dispscan +0x1442C in 0x46E136: only those two reads, the two cancel writes 0x46EBE4 / 0x46EE97 and the tail increment 0x46EF73. `MainMenu_ReplayDraw` and `MainMenu_Calc` do not read it. → entry block never re-runs; value unobservable in sub-states 1–4. | 12 §2 intro (new sentence), 15 §3.8 (entry block line) | right (`menulist.rs` keeps it counting; equivalent). The SPEC-GAP in `replay_run` can be closed as is. |
| 2 | result back-to-list: does anything between the flush and the end of the call play an SE? | 0x456BBB: SE 10, `Sound_Update` until 0, state 12, 72 VMs int 12, frame 0; falls into state-12 code: frame 0 → mkdir + 15 × `Replay_Load`/copy/free; frame < 20 → no input; return 0. `Result_Calc` 0x4584B0 then runs `AnmVm_Run` × 72 (callgraph: no Sound_* callee) and frame += 1. No SE, no BGM command. | 16 §2.6 "Back to the list" | right (`result.rs` flush after the state code is equivalent) |
| 3 | restart: when is the PLST tag 0x164CD0C zero at `Result_Register(2)`? | Writers (dispscan 0x164CD0C): only `Supervisor_OnRegister` 0x445FE5 (:= 'PLST' after zeroing the block, right after `ScoreDat_Load`, before any failure return; `ScoreDat_GetPLST` copies only a chunk tagged PLST). Plus `GameMgr_Ctor` 0x43C5E1 zeroes 0xF78F dwords (covers GM+0x3D804), called only from CRT init 0x4B37A3. `FuncChain_AddCalc` 0x43C880 always calls the on-register; WinMain's test 0x441B87 is reached only after `Supervisor_Register` (window/D3D failure paths skip it). → never zero there; `Result_Register(2)` runs at every exit and restart. | 00 §2 step 1 | right (always flushes) |

CODE BUG list: none.
