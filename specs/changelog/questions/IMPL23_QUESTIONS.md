# IMPL23 questions (result screen, name entry, score.dat, result viewer)

Answered in [answers_re_q24](../answers_re_q24.md).

1. **Default ranking entries' stage byte** (15 §2.3 says stage 0). The ranking prints `(%d)` of the stage byte
   (16 §2.3), and the mopup1 ranking shows `(5)` for an entry whose byte is 5 (score.txt "Stage 5") but `(1)` for every
   default row. Is the default stage 1, or does the ranking map the byte? Port: default byte 1 (SPEC-GAP in `scoredat.rs`).
2. **New-entry flag (list node byte +9)**: is it written to score.dat? Every file read so far has 0 there. Port clears it before
   saving.
3. **State 9 VM interrupts**: which interrupts start the name-entry screen (VM 40 slide-in so the ranking is drawn, the
   difficulty plate VMs 41–46, title VM 48)? Port sends interrupt 3 + difficulty to all 72 VMs at state 9 frame 0.
4. **End panel (state 0xF)**: who sends VM 71 interrupt 18, and when (finish frame, or state 0xF frame 1)? And does the
   "Shot/Enter → interrupt 2" go to VM 71 only or all VMs?
5. **0x164D30C** (end-panel progress): +1 per gameplay frame (which frames: paused, loading, dialogue)? Port: +1 per
   non-paused gameplay frame plus 1800 per op 148.
6. **HSCR +0x78 Last Spells**: where is it incremented (bomb start with the last-spell flag, or elsewhere)?
7. **Human rate counters** (0x164D0A8/0x164D0AC): exact condition "timer > 15" — which timer (enemy manager +?)?
8. **Result_Draw and pos_offset**: the selected menu item is drawn at (−4, −4) (op 24 + op 6 write pos_offset). Does
   Result_Draw add pos_offset to every result00 VM (05 §2 says "added to pos by owner code, INFERRED")?
9. **Text VMs' initial state**: on the confirm frame into play data (state 0x13 at frame 0) mopup1 draws text VMs 11–16
   at (0, 0) and not 0–10. What alpha/position do the 28 text VMs get in Result_OnRegister?
10. **State 8 Up/Down** (`Result_CursorUpDown2`): sound effect? Port plays SE 12.
11. **Menu-page sub-state timing**: "after N sub-frames sub-state 1" — is input accepted on sub-frame N (port) or N+1?
12. **Replay-save dialogue** (states 10–14) is still a stub in the port (straight to state 2). Nothing to answer unless
    16 §2.6 lacks the VM interrupt order for VMs 49–55 (port has not needed it yet).
