# IMPL111 questions

Answered in [answers_re_q46](../answers_re_q46.md).

No new SPEC-GAP markers. Notes for the dirty room (no behaviour depends on them today):

1. 02 §4: which sound command does `Music_Stop` 0x448081 queue (3 or 4)? Both stop the buffers at stage 0, which per
   `0x4728E0` also sets fade mode 0. The port's `Audio::stop_bgm` does not cancel a running fade (dip / fade-in); this
   is inaudible because the buffer is stopped and the next play resets mode and volume, but please confirm.
2. 15 §1.1: the DirectInput keyboard's cooperative level (`SetCooperativeLevel` flags) is not in the spec, so it is
   not stated when `GetDeviceState` fails in practice. The port treats window focus loss as a failed read
   (joystick bits only, held keys dropped) — platform detail, not a marker.
