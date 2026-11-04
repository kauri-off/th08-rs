# IMPL114 questions

Answered in [answers_re_q48](../answers_re_q48.md).

1. `src/audio.rs` `stop_wav` (02 §4 command 3 / 7): `Music_Stop` leaves the buffer stopped at position 0 with the
   playing flag 0; a later command 7 "UnPause" plays it at that position. `GameMgr_OnRemove` queues `Music_Stop` and
   then "UnPause" (11 §7), as do pause-menu / continue paths (12 §3, §4). Is that UnPause reached while the stream
   object still exists, and what is heard — the buffer's stale stream data, silence, or does the streaming thread
   refill it? The port keeps silence until the next play.
2. 02 §4 `Music_PlaySlot`: "both mark track `track` heard unless replay or demo" — does mode 0 also mark (the port
   marks only in modes 1/2, like `Music_PlayFile`)? Relevant for option item 2 switching to mode 0.
