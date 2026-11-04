# IMPL69 questions

1. **Card name +0x7E (15 §3.6 step 1).** Which buffer is copied into the card name? The port uses CATK[spell id] +0x10.
   Is it filled only in spell practice? (`replay.rs` `build_save`.)
2. **USER string tables (15 §3.5).** Which tables give キャラ名, 難易度 and 最終ステージ, and is カード名 `No.%3d` the id or
   id + 1? The port uses 0x4C7F4C (trailing full-width spaces trimmed), 0x4C7FD4, 0x4C7FAC and id + 1. Are the
   ミス回数 / ボム回数 values stats+0x64 / +0x84 printed with `%d`?
3. **Result slot rows (16 §2.6 state 12).** The rows print info fields (+0x6A, +0x6C, +0x72, +0xB0) that are inside the
   compressed, encrypted body. Does state 12 decode each file (Replay_Load-like), or print the raw first 0x134 bytes?
   What does a file with valid magic/version but a bad checksum show? The port decodes and shows the dashes row.
4. **Shot-name table 0x4C8010** (16 §2.6 draw, `%7s`): its 12 strings are not in the specs; the port uses 0x4C86C0.
   Colour of the `No.   Name …` header at VM 55?
5. **0x164D0B0 (11 §2 table).** Which function resets it at death, and does anything set it non-zero? The port resets
   it at the start of `Player_Die` and never sets it.
6. **Clear status +0x11C (15 §3.5).** "else 1 if any Final A clear": the port reads it as 0x43C23B(shot) (this shot's
   any-clear masks E–L & 0x8000), not CLRD entry 12. Confirm.
7. **State 12 cursor on entry.** The cursor is not reset when state 12 is entered (from 10 it is 0; back from 14 it is the
   yes/no cursor; back from 13 by ESC it is the name position, the shared +0x1C). Confirm that +0x28 := cursor runs
   every frame from frame 20 (the port does), so the highlighted slot jumps to that row.
8. **State 12 "at frame 0" (16 §2.6).** Every transition into 12 sets frame := 0, but Result_Calc adds 1 after the state
   body, so the body never sees frame 0. Does `Result_SaveReplay` read the slot headers in the transition itself (the
   port does: `enter_slot_list`, from 10, 13 and 14), or does the state test frame ≤ 1 / another counter?
