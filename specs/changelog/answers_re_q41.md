# RE-Q41 — SPEC-GAP batch 7: input, audio, sound, joystick, ECL interrupt, ascii pause fade, text filter (2026-10-25)

Brief: docs/briefs/RE_Q41_BRIEF.md; markers: docs/specgaps/batch7_input_audio_misc.txt (8). Runs: re/work/runs/q41/
(CHECKPOINT.md). Evidence: disassembly of `Input_Poll` 0x43D970 (all 47 key tests extracted mechanically), the DI
set-up `FUN_00446A37` and its EnumObjects callback 0x445E3D; `Ecl_Run` LOOP test 0x418510 and interrupt handler
0x41C88A–0x41C93C; `Sound_Play` / 13 §1.2 call-site audit (RE-Q25); `Sound_Update` 0x45D790 cases 2/6/7/8;
0x406AC0 / 0x406AF0, 0x406B40 / 0x406B70, 0x406BB0 / 0x406BE0, `Sound_TickBgmFades` 0x447764 (0x472B50, 0x472BE0,
0x472C50, 0x472CC0), `Bgm_SetVolume` 0x472840, constants 0x4B430C (60.0), 0x4B443C (128.0), 0x4B6778 (1000.0);
`PauseMenu_Update` 0x4037B0 (0x404637–0x404641), `ContinueMenu_Update` 0x404890; the MIDI player 0x443C60–0x445440
(decompile re/work/runs/q41/midi.c), `Music_Fade` 0x4480F8, `Music_Stop` 0x448081, `GameMgr_OnRemove` 0x43BE2C, the boot
code that creates the MIDI object; data: scan of the 22 archive .mid files (re/work/runs/q41/scripts/midscan.py).
**Runtime** (one new run, headless Xvfb, own wine prefix): re/work/runs/q41/tri/ — a scratch proxy d3d8.dll (`tri.c`)
that, at the 30th Present of the original, calls th08's own static `D3DXLoadSurfaceFromSurface` 0x47ABAB (filter 4,
colour key 0) on 122 synthetic A1R5G5B5 sources (104 impulses: 96 positions + 8 colour values, random words, random opaque-or-zero,
other sizes) into A4R4G4B4 and A8R8G8B8 512×256 textures; outputs out_4444.bin / out_8888.bin; model scat2.py.

## Changed spec sections
| spec § | change |
|---|---|
| 15 §1.1 | DIK path is the default (DI keyboard unless cfg +0x38 bit 11); GetDeviceState failure → joystick bits only (VERIFIED, was INFERRED); only main Enter on DIK, VK Enter/Shift/Ctrl cover both; joystick DIPROP_RANGE ±1000 on every axis |
| 07 §5.5 | pending test i16 ≥ 0; index k = 32 reads the pending word (sub 32), k ≥ 33 reads following enemy fields |
| 10 §6 | exact source/destination rects of the text copy (always 2:1); the TRIANGLE 2:1 arithmetic (kernel 1,7,7,1 /16, wrap, no alpha weighting, f32 order, rounding) |
| 02 §4 | `Bgm_StartFadeIn` 0x406AC0 (mode 2), `Bgm_StartDip` (mode 4), `Bgm_StartFadeInFromDip` (mode 3): setters, ticks, final values; `Music_Fade` MIDI argument = trunc(1000·secs) ms |
| 02 §5 | new: the MIDI player (object, calls, open/play/stop/fade, 1 ms tick, events, CC 2/CC 4 loop, CC 7 scaling, init.mid uses) |
| 12 §3 | state 10 retry: fade-in VERIFIED (no longer INFERRED) |
| 12 §4 | new "Music" paragraph for the continue menu (Pause / dip on open, Stop / UnPause + fade-in / UnPause on Yes) |
| COVERAGE | MIDI rows → COVERED |

## Markers
| marker | original | spec | port |
|---|---|---|---|
| input.rs:23 key table | Full VK and DIK tables (arrows + keypad 8/2/4/6, keypad 7/9/1/3 diagonals 0x50/0x90/0x60/0xA0, Z, X, Shift, Esc, Ctrl, Q, S, R, Home/P, Enter, D); default path DIK, which maps only main Enter | 15 §1.1 | arrows, X, Shift right; **CODE BUG**: keypad directions/diagonals missing, keypad Enter mapped |
| audio.rs:237 MIDI | own SMF sequencer over midiOut (1 ms timer, tempo meta, CC 2/CC 4 loop, CC 7 × fade factor, ms fade then freeze, init.mid reset) | 02 §5 | **CODE BUG** (MIDI mode plays the WAV track) |
| audio.rs:247 play during fade | play stage 0 stops and cancels the fade, stage 4 sets attenuation 0 (RE-Q30) | 02 §4 | right (play is instant instead of staged over ~7 Sound_Update calls) |
| sound.rs:97 bad id | no range test, but no caller passes an invalid id (sht −1 never played) | 13 §1.2 | right (unreachable) |
| eclvm.rs:414 k ≥ 32 | reads e+0x2CF0+2k: k = 32 → sub 32 (the pending word), larger k → enemy fields; unreachable (data sends k = 1 only) | 07 §5.5 | right (unreachable) |
| text.rs:196 TRIANGLE | exact 2:1, per-axis taps 2j−1…2j+2 weights 1/16, 7/16, 7/16, 1/16 with wrap, channels independent (no alpha weighting), f32 accumulation in source order, trunc(acc·15 + 0.5) into A4R4G4B4 | 10 §6 | **CODE BUG** (alpha-weighted 2×2 box, no wrap, 8-bit output; also its fractional alpha mask vs the 1-bit alpha of 10 §6 step 4) |
| ascii.rs:957 0x406AC0 | `Bgm_StartFadeIn(2.0)`: mode 2, N = n = 120, volume −10000 at once, then −trunc(n·5000/120) per Supervisor calc, ends at −41 without a final write; BGM-volume scaling as all writes | 02 §4, 12 §3 | **CODE BUG** (no fade-in) |
| joystick.rs:58 DIPROP_RANGE | ±1000 on every axis (EnumObjects callback) | 15 §1.1 | right |

## Also found
- Continue menu music (12 §4): opening sends "Pause" (spell practice with a non-restarting card: `Bgm_StartDip(1.0)`,
  music keeps playing at −10 dB); Yes → `Music_Stop`, or UnPause + `Bgm_StartFadeInFromDip(1.0)` (spell practice),
  or UnPause (normal continue). The port never pauses the BGM when the continue menu opens → **CODE BUG**.
- `GameMgr_OnRemove` in MIDI mode plays init.mid after its `Music_Stop` (02 §5).

## CODE BUG list
1. input.rs: keypad 8/2/4/6 and keypad 7/9/1/3 diagonals not mapped; keypad Enter wrongly mapped (DIK default path).
2. audio.rs: MIDI music mode not implemented (02 §5).
3. text.rs: downsample is not the D3DX TRIANGLE 2:1 of 10 §6 (kernel, wrap, unweighted channels, A4R4G4B4 quantisation; and 1-bit alpha).
4. ascii.rs: pause-menu retry in spell practice lacks `Bgm_StartFadeIn(2.0)` (02 §4).
5. (not a marker) continue menu: BGM not paused on open; spell-practice dip / fade-in-from-dip missing (12 §4).
