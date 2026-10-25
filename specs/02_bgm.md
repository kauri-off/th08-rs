# 02 — Music: thbgm.dat and thbgm.fmt

Verified with `tools/thbgm.py` (parses `thbgm.fmt` from the archive, checks every track against
the real `thbgm.dat`, and can write each track as a playable RIFF WAV with `--wav <dir>`).

## 1. thbgm.dat (VERIFIED)

- Raw PCM container, 449 961 024 bytes.
- 16-byte header, checked by the engine when loading the config (`Config_Load` 0x4477B1):

| off | type | value | meaning |
|---|---|---|---|
| 0x00 | u32 | 0x5641575A (`"ZWAV"`) | magic |
| 0x04 | u32 | 1 | version (must be 1) |
| 0x08 | u32 | 0x800 | must be 0x800 |
| 0x0C | u32 | 0 | unused |

  If the file exists but the header mismatches, startup fails with an error. If the file is
  missing and the defaults are applied (no / invalid th08.cfg, 15 §1.2), music mode +0x1F := 2 (MIDI) and a message
  box (0x437CDD) is shown; with a valid cfg the stored mode is kept even when thbgm.dat is missing (VERIFIED RE-Q26,
  `Config_Load` 0x4477B1).
- After the header: raw 16-bit little-endian PCM tracks, back to back, no per-track headers.
- The engine opens it as `./thbgm.dat` (working directory). A global "base offset" added to every
  seek exists (0x18BDC7C) but is never written, i.e. always 0 (VERIFIED via xrefs: reads only).

## 2. thbgm.fmt (inside th08.dat) (VERIFIED)

Loaded once at startup from the archive (`bgm/thbgm.fmt` → entry `thbgm.fmt`, 1105 bytes after
unwrap). Array of 52-byte (0x34) records, terminated by a record whose name starts with NUL
(21 tracks + terminator; trailing bytes are zero).

| off | size | type | meaning |
|---|---|---|---|
| 0x00 | 16 | char[16] | track file name, NUL-padded (e.g. `th08_01.wav`) |
| 0x10 | 4 | u32 | absolute byte offset of the track's PCM in thbgm.dat |
| 0x14 | 4 | u32 | full original track length in bytes (only used as the size of the in-memory preload buffer) |
| 0x18 | 4 | u32 | loop start, bytes relative to track start |
| 0x1C | 4 | u32 | loop end, bytes relative to track start = **bytes actually stored** in thbgm.dat |
| 0x20 | 18 | WAVEFORMATEX | `wFormatTag=1 (PCM), nChannels=2, nSamplesPerSec=44100, nAvgBytesPerSec=176400, nBlockAlign=4, wBitsPerSample=16, cbSize=0` |
| 0x32 | 2 | — | padding |

Observed invariant (VERIFIED for all 21 tracks): `offset[i+1] = offset[i] + loop_end[i]`, and the last
track ends exactly at EOF. So the field at 0x14 is *larger* than the stored data (the part after
the loop end is not shipped); a reimplementation must never read past `loop_end`.

## 3. Playback semantics (VERIFIED from `WaveStream_Rewind` 0x4733E0 / `WaveStream_Read` 0x473530)

- First play: seek to `offset`, stream `loop_end` bytes.
- When the stream runs out it is rewound in loop mode: seek to `offset + loop_start` and stream
  `loop_end - loop_start` bytes; repeat forever. If `loop_start <= 0` the loop restarts from the
  track start.
- Streaming uses a DirectSound buffer of 16 notification chunks, each
  `nSamplesPerSec * 4 * nBlockAlign / 16` bytes rounded down to a multiple of `nBlockAlign`
  (= 44100 bytes ≈ 0.25 s per chunk) (INFERRED meaning of the arithmetic).
- Preload mode: a track can be read fully into memory (`0x14` bytes requested) in a slot (0–15) and
  later started from memory; same loop rules apply to the memory copy.

## 4. How a track is selected

- Scripts and code name music as `bgm/<name>.mid`. `Music_Play(slot, name)` (0x447D56):
  - music mode (config byte) **1 = WAV**: copies the name, replaces the text after the last `.` with
    `wav`, and queues sound command **1 = load** (slot, name) to the sound thread (queue at SoundManager
    +0x1F88, 31 entries × 0x10C: command, slot, flag, name; full queue → dropped). It only loads/prepares the slot;
    nothing is heard yet.
  - mode **2 = MIDI**: opens `bgm/<name>.mid` for the slot (MIDI loader 0x4441A0).
- `Music_PlaySlot(slot, track)` (0x447E47) starts a loaded slot: WAV: (if config +0x38 (0x17CE8A8, debug/driver flags, 15 §1) bit 13: command 4 first) then
  command **2 = play** (slot); MIDI: stop, open slot, play. Both mark music-room track `track` as heard
  (0x164CF14[track] := 1) unless replay playback or demo (VERIFIED, RE-Q24).
  - mode 0: no music.
- `Music_PlayFile(name, track)` (0x447F21, music room, VERIFIED RE-Q39) plays a file directly: mode 2 = MIDI: if a MIDI
  object exists (0x17CE8E4 ≠ 0) stop, open `name`, play; mode 1 = WAV: copy the name, text after the last `.` := `wav`,
  queue sound command **2** with slot −1 and that name. In both modes 0x164CF14[track] := 1 unless replay playback
  (GameFlags bit 3) or demo (bit 1) — in mode 2 even without a MIDI object. Any other mode (0 = off): returns −1 and
  marks **nothing**.
- `Music_Fade(secs)` (0x4480F8, VERIFIED RE-Q25 for WAV mode): MIDI mode → `Midi_Fade(trunc(1000·secs))` (milliseconds, no game-speed division; §5); WAV mode: if the game
  speed (Supervisor+0x188) is ≠ 0 and ≤ 1, secs := secs / speed; queues sound command **5** with the integer
  `trunc(secs)`. The sound thread (`Sound_Update`, after the calc chain) starts the fade on the streaming object:
  mode := 1, N := trunc(float(that integer) · 60), n := N. Every Supervisor calc (`0x447764` from `Supervisor_Calc`,
  before the input poll) then does: n −= 1; if n < 1 → mode 0 and the buffer is **stopped**; else the volume is set
  to `p = n·5000/N − 5000` (integer division; DirectSound hundredths of dB, so the fade is **linear in dB** from
  0 to −50 dB over N frames, not linear in amplitude). Every volume write goes through the BGM-volume scaling
  `0x472840`: BGM volume v (0..100, cfg +0x27 copy 0x18BDC80) = 0 → −10000; else f = 1 − v/100,
  `SetVolume(trunc((p + 5000)·(1 − f²)) − 5000)`.
- **Fade-in** `Bgm_StartFadeIn(secs)` 0x406AC0 (this = SoundManager 0x18B8A68; f32 argument; VERIFIED 0x406AF0 /
  0x472BE0, RE-Q41): only if the streaming object exists (SoundManager +0x5208); runs at once, not through the sound
  queue, in every music mode: fade mode := 2, N := n := trunc(secs · 60.0), `Bgm_SetVolume(−10000)` (through the scaling
  above). Each `Sound_TickBgmFades` (0x447764, every Supervisor calc, same place as mode 1): n −= 1; n < 1 → mode 0 with
  **no** volume write (the last value stays: p = −trunc(1·5000/N), −41 for N = 120, until the next volume write, e.g.
  the next play's `Bgm_SetVolume(0)`); else p = −trunc(n·5000/N). So it rises linearly in dB from −50 dB to ≈ 0.
  Its only caller is the pause menu's retry in spell practice (12 §3 state 10, `secs` = 2.0 → 120 frames), right after
  queueing command 7 "UnPause", which in WAV mode resumes the paused buffer (`Play` at its position, no volume change;
  skipped while the stream is locked) and does nothing in other modes.
  Modes 3 and 4 tick the same way (n −= 1, n < 1 → mode 0 without a volume write) with p = −trunc(n·1000/N) (mode 3)
  resp. trunc(n·1000/N) − 1000 (mode 4); neither stops the buffer. Setters, both immediate with N = n = trunc(secs·60):
  `Bgm_StartDip(secs)` 0x406B40 → 0x406B70 mode 4, **no** initial volume write (0 → −10 dB over N frames, then it stays
  at trunc(1000/N) − 1000, −984 for N = 60); `Bgm_StartFadeInFromDip(secs)` 0x406BB0 → 0x406BE0 mode 3,
  `Bgm_SetVolume(−1000)` first (−10 dB → ≈ 0 over N frames, last value −trunc(1000/N) = −16 for N = 60). Only caller of
  both: the continue menu in spell practice (12 §4 "Music"), secs = 1.0. A later setter simply overwrites mode/N/n.
- Play during a fade (VERIFIED `Sound_Update` 0x45D790 case 2, RE-Q30): `Sound_Update` handles the queue head; a
  play command is staged, one stage per call (head stage counter +8 += 1), and leaves the queue when done. Usual path
  (Supervisor +0x150 bit 13 clear, or slot < 0): stage 0 **stop** the buffers (`0x4728E0`, also fade mode := 0, so a
  running `Music_Fade` is cancelled as soon as the play command reaches the head), 1 recreate (repeated while the
  stream is locked), 2 reopen the track, 3 fill the buffer, 4 **play** (`0x472750`: fade mode := 0, n := N := 0,
  `Bgm_SetVolume(0)` — attenuation back to 0 — then start), done after stage 6. With bit 13 set (preloaded slots):
  0 `Bgm_LoadPreloaded`, 2 reset, 5 fill, 7 play (same `0x472750`); there is no stop stage, so the old fade keeps
  ticking in `Supervisor_Calc` (and may stop the buffer) until stage 7. Commands 3/4 also stop the buffers at stage 0.
- `Bgm_FindTrack` (0x45C6B0): takes the basename (after last `/` or `\`), compares it with the
  record names case-sensitively; **if no record matches, index 0 (`th08_01.wav`) is used**.

### Track table (VERIFIED values; titles from `musiccmt.txt` in the archive)

| idx | file | offset | stored bytes (=loop end) | loop start | loop start / end (s) | music-room No. / title |
|---|---|---|---|---|---|---|
| 0 | th08_01.wav | 0x00000010 | 12715264 | 989888 | 5.61 / 72.08 | No.1 永夜抄　～ Eastern Night. |
| 1 | th08_00.wav | 0x00C20510 | 23128832 | 699904 | 3.97 / 131.12 | No.2 幻視の夜　～ Ghostly Eyes |
| 2 | th08_03.wav | 0x0222F010 | 13762560 | 2785280 | 15.79 / 78.02 | No.3 蠢々秋月　～ Mooned Insect |
| 3 | th08_04.wav | 0x02F4F010 | 26398720 | 487168 | 2.76 / 149.65 | No.4 夜雀の歌声　～ Night Bird |
| 4 | th08_05.wav | 0x0487C010 | 16978176 | 4511360 | 25.57 / 96.25 | No.5 もう歌しか聞こえない |
| 5 | th08_06.wav | 0x058AD110 | 16285696 | 2740736 | 15.54 / 92.32 | No.6 懐かしき東方の血　～ Old World |
| 6 | th08_07.wav | 0x06835110 | 20195840 | 4761088 | 26.99 / 114.49 | No.7 プレインエイジア |
| 7 | th08_08.wav | 0x07B77B10 | 16809984 | 1460992 | 8.28 / 95.29 | No.8 永夜の報い　～ Imperishable Night. |
| 8 | th08_09.wav | 0x08B7FB10 | 28934144 | 2337536 | 13.25 / 164.03 | No.9 少女綺想曲　～ Dream Battle |
| 9 | th08_10.wav | 0x0A717B10 | 22355968 | 1813504 | 10.28 / 126.73 | No.10 恋色マスタースパーク |
| 10 | th08_11.wav | 0x0BC69B10 | 30779648 | 1178368 | 6.68 / 174.49 | No.11 シンデレラケージ　～ Kagome-Kagome |
| 11 | th08_12.wav | 0x0D9C4410 | 21200384 | 534144 | 3.03 / 120.18 | No.12 狂気の瞳　～ Invisible Full Moon |
| 12 | th08_13.wav | 0x0EDFC210 | 15250592 | 1665728 | 9.44 / 86.45 | No.13 ヴォヤージュ1969 |
| 13 | th08_14.wav | 0x0FC876B0 | 31425536 | 1235712 | 7.01 / 178.15 | No.14 千年幻想郷　～ History of the Moon |
| 14 | th08_13b.wav | 0x11A7FAB0 | 14449664 | 12362624 | 70.08 / 81.91 | No.16 ヴォヤージュ1970 |
| 15 | th08_15.wav | 0x128476B0 | 31748096 | 6678272 | 37.86 / 179.98 | No.15 竹取飛翔　～ Lunatic Princess |
| 16 | th08_16.wav | 0x1468E6B0 | 15827456 | 3086848 | 17.50 / 89.72 | No.19 月見草 |
| 17 | th08_17.wav | 0x155A68B0 | 18601216 | 3434880 | 19.47 / 105.45 | No.20 Eternal Dream　～ 幽玄の槭樹 |
| 18 | th08_18.wav | 0x16763DB0 | 28015744 | 881280 | 5.00 / 158.82 | No.17 エクステンドアッシュ　～ 蓬莱人 |
| 19 | th08_19.wav | 0x1821BA30 | 31184784 | 1334816 | 7.57 / 176.78 | No.18 月まで届け、不死の煙 |
| 20 | th08_20.wav | 0x19FD91C0 | 13912704 | 1104896 | 6.26 / 78.87 | No.21 東方妖怪小町 |

### Where each track is used

Sources: the three BGM name slots in each `.std` header (offsets 0x290, 0x310, 0x390, 128 bytes each,
`' '` = empty; slot 0 is started at stage start, slots 1/2 are preloaded — VERIFIED field offsets from
the stage-start code in FUN_0043ABD7, purpose of slots 1/2 INFERRED as boss/alternate themes), strings
in `.end` files, and literals in the exe.

| context | file(s) |
|---|---|
| Title / main menu, replay & result screens | `th08_01` (literal `bgm/th08_01.mid` in FUN_0046837B and FUN_00470E10) |
| Stage 1 | `th08_00` (stage), `th08_03` (boss) |
| Stage 2 | `th08_04`, `th08_05` |
| Stage 3 | `th08_06`, `th08_07` |
| Stage 4A (uncanny) | `th08_08`, `th08_09` |
| Stage 4B (powerful) | `th08_08`, `th08_10` |
| Stage 5 | `th08_11`, `th08_12` |
| Stage 6A (Eirin) | `th08_13`, `th08_14`, `th08_13b` |
| Stage 6B (Kaguya) | `th08_13`, `th08_15`, `th08_13b` |
| Extra | `th08_18`, `th08_19` |
| Endings (`end0??.end`) | `th08_16` |
| Staff roll (`staff00*.end`) | `th08_17` |
| Spell-card practice mode | from a table in the exe (0x4C7670, 20-byte records `{max_spell_id, music-room track, name*, ?, retry-restart flag}`: +4 is the `Music_PlaySlot` track of 11 §4 step 3; +0x10 = 1 only for the two th08_13b records, read by `FUN_00439916` for the retry music rules of 11 §4 step 3 and 12 §3 state 10, VERIFIED RE-Q36); the first record whose `max_spell_id >= current spell id` is used. Thresholds: ≤1 th08_00, ≤12 03, ≤16 04, ≤31 05, ≤35 06, ≤53 07, ≤76 09, ≤99 10, ≤118 12, ≤122 13, ≤142 14, ≤146 13b, ≤150 13, ≤170 15, ≤190 13b, ≤193 18, ≤204 19, ≤222 **th08_20** (Last Words). (Table VERIFIED; "spell practice" meaning INFERRED from the gate flag and spell index.) |
| Music room | all 21, in `musiccmt.txt` order |

Notes:
- `th08_02` does not exist in thbgm.fmt; `stage1_s.std` names it, and `stage5_s.std` names
  `th08_13_b` (typo). Under the lookup rule these resolve to index 0 (`th08_01`). The `_s.std`
  variants appear to be used in the spell-practice path where the exe table above overrides the
  std names (INFERRED).

## 5. MIDI mode (music mode 2) — VERIFIED (RE-Q41, 0x443C60–0x445440)

The MIDI player is an own SMF sequencer that sends events through winmm `midiOut`; `thbgm.dat` and the BGM volume
(cfg +0x27) are not used (no MIDI function reads the volume copy 0x18BDC80). Files are archive members
`bgm/th08_NN.mid` (the names scripts pass, unchanged) and `bgm/init.mid`; all 22 are format 0, one track, division
480, and every song marks its loop with **CC 2** (loop start) and **CC 4** (loop end) (scan RE-Q41, e.g. th08_01:
CC 2 at tick 3840, CC 4 at 77762, end of track 81610).

**Object** (0x300 bytes, `Midi_Ctor` 0x443FA0, pointer 0x17CE8E4, created at boot in every music mode, freed at exit):
32 file slots +0x98 (raw file data, `Midi_LoadSlot` 0x4441A0: if the slot is the open one, `Midi_Stop` first; load via
`FileSystem_OpenFile(name, 0)`); open slot +0x10 (−1 none); track count +0x118, format +0x11C, division +0x120, tempo
+0x124 (µs per quarter note), elapsed timer ticks T +0x128 (u64), tick base B +0x130 (u64), track array +0x138
(0x20 each: +0 active, +4 next event time (ticks, absolute), +8 length, +0xC running status, +0x10 data copy,
+0x14 read pointer, +0x18/+0x1C loop-start copies of +0x14/+4), device +0x13C (HMIDIOUT, device id +0x140), 32 SysEx
header slots +0x14 with ring index +0x94, per-channel state +0x154 + ch·0x17 (16 B note bitmap, +0x10 program, +0x11
bank CC 0, +0x12 pan CC 10, +0x13 CC 91, +0x14 CC 93, +0x15 raw CC 7, +0x16 sent CC 7), key offset +0x2C4 (u8, always
0), volume factor +0x2C8 (f32), last trunc(factor·128) +0x2CC, CC 7 refresh disable +0x2D4 (always 0), fade on +0x2E0,
fade length +0x2E4 / counter +0x2E8 (timer ticks), loop-start copies of tempo/T/B +0x2EC…+0x2FC.

**Calls.** Boot: `Midi_LoadSlot(30, "bgm/init.mid")`. `Music_Play(slot, name)` mode 2 → `Midi_LoadSlot(slot, name)`.
`Music_PlaySlot` mode 2 → `Midi_Stop`, `Midi_Open(slot)`, `Midi_Play`. `Music_PlayFile` mode 2 → `Midi_Stop`,
`Midi_PlayFileOnce(name)` 0x444510 (load into slot 31, `Midi_Open(31)`, free slot 31's file — the parsed copy stays),
`Midi_Play`. `Music_Stop` mode 2 → `Midi_Stop`. In mode 2, `Midi_Stop`, `Midi_Open(30)`, `Midi_Play` (init.mid = the reset sequence: SysEx and
controller set-up, then a 2-tick CC 2/CC 4 loop that holds the device silent) run at: main-menu Quit (12 §2),
`GameMgr_OnRemove` 0x43BE2C right after its `Music_Stop` (run unless spell practice (GameFlags bit 14) and the next scene is 3, 11 or 12), and 0x446D06 (Supervisor teardown).
- `Midi_Open(slot)` 0x4442E0: frees the parsed tracks, reads the header (big-endian format, track count, division at
  file +8 after the `MThd` length), copies every `MTrk` body into its own buffer, tempo := 1 000 000, open slot := slot.
- `Midi_Play` 0x444620: `Midi_Rewind` 0x444550 (factor := 1.0, fade off, T := B := 0, every track active, read pointer
  := start, next time := first delta), `midiOutOpen(MIDI_MAPPER = −1)` (kept if the same id is already open), then
  `timeSetEvent(1 ms, resolution = timeGetDevCaps wPeriodMin, periodic)` after `timeBeginPeriod`.
- `Midi_Stop` 0x444680: unprepares/frees the SysEx headers, kills the timer, `midiOutReset` + `midiOutClose`, open slot
  := −1.
- `Midi_Fade(N)` 0x4447D0: factor := 0, fade length := N, counter := 0, fade on.

**Timer tick** `Midi_Tick` 0x444820 (every 1 ms timer callback; time is counted in callbacks, not wall time):
1. pos := B + trunc(T·division·1000 / tempo) (unsigned 64-bit).
2. If fading: counter ≥ length → factor := 0 and **return** (nothing else runs any more: the song is frozen, notes
   that were on keep sounding until `Midi_Stop`/`midiOutReset`). Else factor := 1 − counter/length (f32); if
   trunc(factor·128) ≠ last value → resend CC 7 on all 16 channels with clamp(trunc(raw CC 7 · factor), 0, 127);
   last := trunc(factor·128); counter += 1.
3. For each active track in order, while it is active and its next time ≤ pos: run one event (below), then recompute
   pos as in step 1 (a tempo event changes it).
4. T += 1. If no track was active at step 3 (all ended), `Midi_Rewind` (the song restarts from the top; the fade is
   cancelled and the factor reset to 1).

**Event** `Midi_Event` 0x444A90: status byte (< 0x80 → running status +0xC); 0x8n/0x9n/0xAn/0xBn/0xEn read 2 data
bytes, 0xCn/0xDn one. 0xF0 SysEx: length VLQ, data copied behind an 0xF0 byte into a fresh header in the ring slot
(old one unprepared first), `midiOutPrepareHeader` + `midiOutLongMsg`. 0xFF meta: type, length VLQ; 0x2F end of track
→ track inactive, return; 0x51 tempo → B := current pos (step 1), T := 0, tempo := the big-endian data bytes; other
metas skipped. Bookkeeping: note on (velocity ≠ 0) / off set / clear the note bit (key + offset 0); CC 0/10/91/93 and
program are recorded; CC 7 stores the raw value and **sends** clamp(trunc(raw · factor), 0, 127) instead; **CC 2**
saves every track's read pointer and next time and the tempo, T and B (loop start); **CC 4** restores them (loop end:
playback continues with the event after the CC 2, with the CC 2's time and timing state). Every channel message
(status < 0xF0, CC 2/CC 4 included) is sent with `midiOutShortMsg`. Then running status := status, next time += the
following delta.

