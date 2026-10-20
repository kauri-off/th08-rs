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
- `Music_Fade(secs)` (0x4480F8, VERIFIED RE-Q25 for WAV mode): MIDI mode → `Midi_Fade` (argument INFERRED); WAV mode: if the game
  speed (Supervisor+0x188) is ≠ 0 and ≤ 1, secs := secs / speed; queues sound command **5** with the integer
  `trunc(secs)`. The sound thread (`Sound_Update`, after the calc chain) starts the fade on the streaming object:
  mode := 1, N := trunc(float(that integer) · 60), n := N. Every Supervisor calc (`0x447764` from `Supervisor_Calc`,
  before the input poll) then does: n −= 1; if n < 1 → mode 0 and the buffer is **stopped**; else the volume is set
  to `p = n·5000/N − 5000` (integer division; DirectSound hundredths of dB, so the fade is **linear in dB** from
  0 to −50 dB over N frames, not linear in amplitude). Every volume write goes through the BGM-volume scaling
  `0x472840`: BGM volume v (0..100, cfg +0x27 copy 0x18BDC80) = 0 → −10000; else f = 1 − v/100,
  `SetVolume(trunc((p + 5000)·(1 − f²)) − 5000)`. Fade modes 2/3 (same tick, volume −n·5000/N resp. −n·1000/N,
  no stop) belong to other commands (INFERRED fade-ins; not used by `Music_Fade`).
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
| Spell-card practice mode | from a table in the exe (0x4C7670, 20-byte records `{max_spell_id, ?, name*, ?, ?}`); the first record whose `max_spell_id >= current spell id` is used. Thresholds: ≤1 th08_00, ≤12 03, ≤16 04, ≤31 05, ≤35 06, ≤53 07, ≤76 09, ≤99 10, ≤118 12, ≤122 13, ≤142 14, ≤146 13b, ≤150 13, ≤170 15, ≤190 13b, ≤193 18, ≤204 19, ≤222 **th08_20** (Last Words). (Table VERIFIED; "spell practice" meaning INFERRED from the gate flag and spell index.) |
| Music room | all 21, in `musiccmt.txt` order |

Notes:
- `th08_02` does not exist in thbgm.fmt; `stage1_s.std` names it, and `stage5_s.std` names
  `th08_13_b` (typo). Under the lookup rule these resolve to index 0 (`th08_01`). The `_s.std`
  variants appear to be used in the spell-practice path where the exe table above overrides the
  std names (INFERRED).
