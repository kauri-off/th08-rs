# IMPL112 questions

Answered in [answers_re_q46](../answers_re_q46.md).

SPEC-GAP markers added (7; total 13 → 19, the old `audio.rs:237` MIDI marker closed):

1. `midi.rs` `Sequencer::new` — 02 §5: initial values of +0x2CC (last trunc(factor·128)) and the raw CC 7 bytes
   (+0x154 + ch·0x17 + 0x15) from `Midi_Ctor`, and whether `Midi_Open` / `Midi_Play` reset them. Port: 128 and 0.
   Inaudible for the shipped songs (every channel with notes sets CC 7).
2. `midi.rs` `tick` step 4 — "if no track was active at step 3": tested before step 3 runs (the port: a song whose
   last track ends in tick k rewinds in tick k+1) or after it (rewind in tick k)?
3. `midi.rs` `restore_loop` — CC 4 before any CC 2: the copies hold their ctor values; what does the restore do?
   Port: plain controller. Not reachable with the shipped files.
4. `midi.rs` `event` — status 0xF7 and 0xF1–0xFE: how does `Midi_Event` read them? Port: F7 length-prefixed skip,
   others no data. Not in the shipped files.
5. `audio.rs` `midi_load_slot` — `Midi_LoadSlot` / `Midi_Open` with a file missing from the archive (`th08_02.mid`
   named by `stage1_s.std`, `th08_13_b.mid` by `stage5_s.std`): what happens (FileSystem_OpenFile null → slot
   empty → Midi_Open on null?). Port: nothing plays.
6. `audio.rs` `play_bgm` — switching music mode in the options (12 §2.7, 15 §1.3): does a playing WAV stream stop
   when mode 2 starts the MIDI title theme, and does a MIDI song stop when mode 1 starts the WAV one (Music_Play /
   PlaySlot only call the path of the current mode)? Port: the other path is stopped.
7. `audio.rs` `pause_bgm` — sound command 6 "Pause" in mode 2 (pause menu, continue menu): 02 §4 says command 7 does
   nothing outside WAV mode; is command 6 also a no-op, so the MIDI song keeps playing while paused? Port: yes.

Other notes (no marker):

8. Spec conflict: 11 §4 step 6 (demo fade) says "MIDI: Midi_Fade(3)", 02 §4/§5 say `Midi_Fade(trunc(1000·secs))`
   (3000 ms). The port follows 02.
9. 04 file index says init.mid is "played at start"; 02 §5 lists only `Midi_LoadSlot(30, "bgm/init.mid")` at boot.
   The port loads it only. Is it also played at boot?
10. 10 §5 op 12 / 16 §1.3 `@M` / music room Ctrl: `Music_Fade` divides by the game speed internally in WAV mode
    (02 §4), so `Audio::fade_bgm` now takes the speed for every caller (the demo caller used to divide itself).
11. Port detail, not a marker: rustysynth has no SysEx input; GM / GS / XG system-reset SysEx map to a synth reset,
    other SysEx (init.mid's GS parameter set-up) are dropped; `midiOutReset` → all notes off with release.
