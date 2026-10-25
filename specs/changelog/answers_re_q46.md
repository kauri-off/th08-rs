# RE-Q46 — IMPL111 questions 1–2, IMPL112 questions 1–11 (Music_Stop, keyboard cooperative level, MIDI details)

Code only (decompile + capstone disasm of th08.exe, .rdata reads) and two scans of the shipped .mid files
(re/work/runs/q46/midiscan.py: tempo metas, Fx statuses; sysex.py: SysEx list). No runs of the original.

| # | question | original | spec | port |
|---|---|---|---|---|
| 1 (IMPL111) | `Music_Stop` command 3 or 4; does stop cancel a fade? | 0x448081: mode 2 `Midi_Stop`; mode 1 command 3 (Supervisor+0x150 bit 13 clear) / 4 (set). Both stage 0 `Bgm_StreamStop` 0x4728E0: `Stop` + `SetCurrentPosition(0)` per buffer, +0x1C fade mode := 0, +0x30 := 0. Command 4 also stops the thread and deletes the stream object (stages 1–3). | 02 §4 (new `Music_Stop` + stream-field + command 6/7 bullets) | right in effect: `audio.rs` `stop_bgm` leaves `self.fade` running, unobservable (every later play resets it). Clear it for state parity. |
| 2 (IMPL111) | keyboard cooperative level | `FUN_00446A37`: `SetCooperativeLevel(hwnd, 0x16)` = NONEXCLUSIVE \| FOREGROUND \| NOWINKEY, one `Acquire`; joystick 0x0A (NONEXCLUSIVE \| BACKGROUND). | 15 §1.1 (new "Device set-up" paragraph), COVERAGE | right (focus loss = failed read, pad bits only) |
| 3 (IMPL112 1) | initial +0x2CC / raw CC 7; reset by Open/Play? | `Midi_Ctor` 0x443FA0 zeroes +0x2C4…+0x2E0 (+0x2CC = 0, factor 0.0); per-channel state +0x154… not written (heap from `operator new(0x300)`). Only `Midi_Tick` writes +0x2CC, only `Midi_Event` writes channel state; Open/Play/Rewind/Fade/Stop reset neither → both carry over between fades and songs. | 02 §5 (Object paragraph) | CODE BUG, inaudible: `midi.rs` `Sequencer::new` sets `last_q` 128 and `cc7` 0 per song; original 0 at boot and carried across songs (so the first fade tick after boot always resends CC 7). |
| 4 (IMPL112 2) | step 4 "no track active": before or after step 3 | 0x444978 tests track +0 when the loop reaches it, 0x444982 sets the flag before any event runs → a track ending in tick k counts in tick k; rewind in tick k + 1. | 02 §5 step 4 | right |
| 5 (IMPL112 3) | CC 4 before any CC 2 | Restores per-track +0x18 (= track start, written by every `Midi_Rewind`) / +0x1C (0 after `Midi_Open` zeroes the track array, not reset by Rewind) and tempo/T/B +0x2EC…+0x2FC (written only by CC 2: last CC 2 of any song since boot, indeterminate before). The CC 4's delta read then lands on the first event. | 02 §5 (CC 4 sentence) | CODE BUG, unreachable: `midi.rs` `restore_loop` is a no-op without a CC 2. |
| 6 (IMPL112 4) | 0xF7 / 0xF1–0xFE | `Midi_Event` 0x444A90 case 0xF0: only 0xF0 (SysEx) and 0xFF (meta) read bytes; every other 0xFn reads nothing and sends nothing; running status := status for every status. No 0xFn other than F0/FF in the shipped files. | 02 §5 (Event) | CODE BUG, unreachable: `midi.rs` `event` skips a VLQ length after 0xF7. Others right. |
| 7 (IMPL112 5) | missing file (`th08_02.mid`, `th08_13_b.mid`) | `Midi_LoadSlot` 0x4441A0: `FileSystem_OpenFile(name, 0)` null → log, slot null, −1. `Midi_Open` frees the parsed tracks first, then −1 (open slot unchanged); `Midi_Play`/`Midi_Stop` return −1 when no tracks → silence. (`Music_PlayFile` with a failed load would restart the previous song — unreachable.) | 02 §5 (two new bullets) | right (silent) |
| 8 (IMPL112 6) | option BGM change: other mode's music stopped? | `MainMenu_Option` 0x46837B item 2: `Music_Stop` in the **old** mode, Left from mode 2 also plays init.mid, then the mode changes and `Music_Play(8, th08_01)` + `Music_PlaySlot(8, 0)` run in the new mode. Item 7 Reset sets mode 1 with no music call; WAV paths never call MIDI functions, so a MIDI song keeps looping under the WAV music. | 15 §1.3 rows 2 and 7, 12 §2.7 ref | item 2 right (Left 2 → 1 init.mid missing, inaudible). CODE BUG, reachable: Reset from Midi — `audio.rs` `play_wav` calls `midi_stop`, so the port ends the MIDI song at the next WAV play; the original keeps it playing. |
| 9 (IMPL112 7) | command 6 in mode 2 | `Sound_Update` case 6/7: `if (mode == 1)` only; no MIDI call anywhere in the pause paths (full `Midi_*` caller list checked) → the MIDI song keeps playing. Mode 1 detail: 6 = `0x472990` (+0x30 := 0, `Stop`, fade mode **kept**), 7 = `0x4729E0` (+0x30 := 1, `Play`, no volume write). | 02 §4 (command 6/7 bullet) | right for mode 2. CODE BUG, near-inaudible: a mode-1 fade that ends during a pause — the original resumes the buffer at p = trunc(5000/N) − 5000 (−4980 for 240 frames) on UnPause; the port's `tick_fade` end runs `stop_wav`, so nothing resumes. |
| 10 (IMPL112 8) | 11 §4 demo fade "Midi_Fade(3)" vs 02 | 0x43A40D `Music_Fade(3.0)`; 0x448115 `fld [0x4B6778] = 1000.0; fmul; ftol` → `Midi_Fade(3000)`. | 11 §4 step 6 fixed | right |
| 11 (IMPL112 9) | init.mid played at boot? | `Supervisor_Init` 0x4464C8 only `Midi_LoadSlot(30)`. `Midi_Open(30)` callers: GameMgr_OnRemove, 0x446D06, MainMenu_Top case 8 (Quit), MainMenu_Option item 2 Left. | 04 file index fixed, 02 §5 Calls | right |
| 12 (IMPL112 10) | `Music_Fade` divides by speed internally | 0x44813D–0x448185: WAV mode reads Supervisor+0x188 (= g_GameSpeed 0x17CE8E0), divides when ≠ 0 and ≤ 1; callers (Ending_RunScript 0x4D, Msg_Run op 12 = 4.0, demo 3.0, MusicRoom Ctrl 8.0) pass plain seconds. | 02 §4 (unchanged) | right |
| 13 (IMPL112 11) | SysEx with rustysynth | Every SysEx is sent unchanged (`midiOutLongMsg`, 0xF0 + the file bytes incl. the closing F7) to MIDI_MAPPER. All songs, not only init.mid, carry Roland GS SysEx (GS reset in every file, reverb/chorus/insertion-effect and part parameters). | 02 §5 (end of Event) | port detail (device-dependent), acceptable |

New outside the questions (reachable): **tempo meta bug.** `Midi_Event` 0x444D97–0x444DB7 builds the tempo as
`tempo = (tempo << 8) + b + tempo` per data byte = tempo·257 + b. Every shipped song has tempo metas (scan: e.g.
th08_01 431655 → 434883 µs, th08_00 397351 → 400445), so the original plays every MIDI song ≈ 0.75 % slower than the
file. 02 §5 Event. Port: `midi.rs` `event` 0x51 uses `(v << 8) | b` — CODE BUG.

## CODE BUG list
Reachable:
1. `midi.rs` `event` 0x51: tempo must be `v·257 + b` per byte (02 §5), all MIDI songs ≈ 0.75 % too fast now.
2. Option Reset (item 7) from Midi: the MIDI song must keep playing in WAV mode; `audio.rs` `play_wav`'s `midi_stop`
   (and `stop_bgm`'s `midi_stop` in mode 1) must not run outside mode 2 (15 §1.3 row 7, 02 §4 `Music_Stop`).
3. Near-inaudible: WAV pause (command 6) keeps the fade; a mode-1 fade that ends while paused stops the buffer, and
   UnPause plays it again at the last fade volume (02 §4 command 6/7) — the port drops the track instead.

Inaudible / unreachable (state parity):
4. `midi.rs` `Sequencer::new`: +0x2CC initial 0 at boot, kept across songs, likewise the raw CC 7 bytes (02 §5 Object).
5. `midi.rs` `restore_loop` without a CC 2 (02 §5 CC 4 sentence); `event` 0xF7 must read no length (02 §5 Event).
6. `menu.rs` option item 2 Left from Midi: init.mid reset (`midi_reset` before the mode change, 15 §1.3 row 2);
   `audio.rs` `stop_bgm` should also clear the fade (02 §4 `Music_Stop`).
