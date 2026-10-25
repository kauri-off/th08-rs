# RE-Q48 — IMPL114 questions 1–2 (UnPause after Music_Stop, Music_PlaySlot heard flag)

Code only (decompile + capstone disasm of th08.exe; msg/ecl dumps re/extracted/msg_dump.txt, ecl_dis.txt). No runs of the
original — the stream logic settles Q1 without one.

| # | question | original | spec | port |
|---|---|---|---|---|
| 1 | UnPause (command 7) after `Music_Stop`: object alive? what is heard? | Command 3 (`Sound_Update` 0x45D790 case 3) only runs `Bgm_StreamStop` 0x4728E0 (Stop + position 0, +0x30 := 0, +0x1C := 0); the object (+0x5208) survives (only command 4 deletes it). Command 7 → 0x4729E0: +0x30 := 1, `Play(0, +0x20, +0x24)` = looping from 0. Ring (`Bgm_StreamStart` 0x45CC70 → 0x4718D0: 16 chunks of 44100 B, notification at k·chunk + chunk − 1; thread 0x45E0F0 handles events only while +0x30 ≠ 0; handler 0x472D30 refills the chunk at +0x64 unless the write cursor is inside it, "Stream Skip") is neither cleared nor reset by the stop; +0x64 = P = chunk playing at the stop, file 4.0 s ahead. → music returns at once at the last volume, seamless from song time t + ((16 − P) mod 16)·0.25 s (P = 1: 0.25 s of t + 3.75 s, then t on), until the next play. Sites: no path queues 7 after its own `Music_Stop` (pause retry / continue retry branch on one or the other; `GameMgr_OnRemove` 0x43BE2C queues `Music_Stop` only and drains the queue with `Sound_Update` until 0). Reachable via msg6a–d / msg7a–d entry 4 (`bgm −1`, timeline ecldata6 t4023 / ecldata7 t4113) then pause-close or continue during entry 3 (its `bgm 2` is at its end). Cfg bit 13: case 6/7 dereference the null object (+0x74) — crash, debug flag only. | 02 §3 "Streaming ring" (new), §4 "UnPause after `Music_Stop`" (new), stream-object fields; 10 §5 op 7; 11 §7 on-remove; 12 §3 state 4, §4 Music | CODE BUG, reachable/audible: `audio.rs` `stop_wav` drops the music (silence until next play). Command 3 must keep the WAV stream so that UnPause resumes it as described. |
| 2 | `Music_PlaySlot` mode 0: marks heard? | 0x447EB2 `cmp mode, 1; jne 0x447F1B` → `xor eax, eax; leave`. Stores to 0x164CF14 only at 0x447EA7 (mode 2, also without a MIDI object) and in the mode-1 branch. Mode 0 marks nothing. | 02 §4 `Music_PlaySlot` | right (`menu.rs` option item 2 marks only in modes 1/2); the SPEC-GAP can be closed as is. |

## CODE BUG list
1. `audio.rs` `stop_wav` / `stop_bgm` (command 3): must not discard the WAV stream; a later UnPause (`pause_bgm(false)`)
   plays it on per 02 §4 "UnPause after `Music_Stop`" (continuation 0–3.75 s past the stop point, at the last volume)
   until the next play. Reachable: stage 6/7 pause (or continue) during the dialogue between `bgm −1` and `bgm 2`.
