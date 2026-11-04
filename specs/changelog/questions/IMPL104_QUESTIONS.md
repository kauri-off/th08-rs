# IMPL104 questions

1. 12 §3 state 10 (pause retry, spell practice, card record flag 0): "sound command 7 "UnPause" and `0x406AC0(2.0)`
   on the BGM stream (state 2, volume −10000: a 2 s fade-in, INFERRED)". What exactly does `0x406AC0` do (fade
   mode, frame count N = 2.0 · 60?, start volume, per-frame volume formula, interaction with the BGM-volume scaling
   `0x472840`)? The port only unpauses (marker in `src/ascii.rs` `PauseMenu::update`).
