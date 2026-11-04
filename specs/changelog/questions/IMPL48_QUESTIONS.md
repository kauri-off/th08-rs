# IMPL48 questions

Answered in [answers_re_q30](../answers_re_q30.md).

1. **SE off and the BGM command queue (13 §1.3, 02 §4).** 13 §1.3 says `Sound_Update` is "skipped entirely" when
   cfg "SE on" is 0, and that the BGM command queue (+0x1F88) "is processed in the same call". Is the BGM queue
   (commands 1 load, 2 play, 5 fade) also skipped with SE off, or only the SE flush? The port starts a queued fade
   regardless of SE on (`game.rs` `sound_update`, SPEC-GAP).
2. **New track during a fade (02 §4).** If `Music_Play` / `Music_PlaySlot` runs while a `Music_Fade` is counting
   down, does the fade (mode 1) keep running and stop the new track, and is the new buffer's volume reset from the
   faded value? The port cancels the fade and resets the attenuation to 0 on every play (`audio.rs` `play_bgm`,
   SPEC-GAP).
