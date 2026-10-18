# Answers to IMPL17_QUESTIONS — gameplay/demo part (Q2, Q5) (dirty room, 2026-10-18)

The menu questions (Q1, Q3, Q4, Q6, Q7) belong to the next RE run.
Sources: th08.exe, the reference draw logs `specs/data/drawlog/demos/`, the session recordings `sessions/golden` and
`sessions/selftest1`, and the attract-demo replays.

Spec sections fixed in place:
- 11 §3.1: loading frame count and per-frame depth clear.
- 12 §5.1: clock VM gating.
- 15 §3.3: playback fps text.
- 06 §11.3: ribbon details.

## 2. Stage loading frame count; depth clear

### 2.1 Loading frames — VERIFIED (code) / measured

The stage loader thread (`GameManager_StageStart`) ends with these steps:
1. Wait while `loading_frames` (GM+0x3C, incremented by every loading `GameMgr_Calc`) is < 30, or < 80 in replay
   playback, polling with `Sleep(17)`.
2. `Loading_Wait`.
3. Wait until flags bits 5–6 are 0 (stage-clear confirm).
4. `loading = 0`.

The thresholds give **31 loading calcs (81 in replay)** when the loader's own work is shorter than that. The demo
traces show 81. In the golden session, stage 2 and stage 3 reach `Loading_Wait` 31 calcs after loading started
(then they wait for the clear-screen confirm).

The loader's work has a floor, set by the texture uploads. Each `Anm_LoadAndWait` waits until the main thread has
uploaded every entry of the file: `Supervisor_Calc` → `AnmManager_PostloadPending` → `Anm_PostloadStep`, **one
entry per frame**. The rest is host-dependent decoding time.

Measured: a fresh start of stage 1 (Reimu & Yukari, sessions `golden` and `selftest1`) took **48** loading calcs in
both recordings. That start loads about 41 entries: stg1bg 1, eff01 2, enemy 1, stg1enm 1, etama 6, front 2, times 1,
stg1txt 1, player00 2, faces ≈ 24.

Suggested port rule: `max(31 | 81, total entries uploaded + a few frames)`. When comparing against a session, use
the recorded `release stage poll` (README "Loader threads"). The 30 the port uses for non-replay starts is one short:
the minimum is 31.

### 2.2 Full-screen depth clear — VERIFIED (code + draw logs)

The `C z` at the start of each gameplay frame comes from `GameMgr_Calc` itself: `Clear(flags = Z, depth 1.0)` on the
device's current viewport. That is the full screen left by the previous frame; every logged gameplay frame shows
V 0 0 640 480.

It runs on every calc that gets past the loading test. That includes paused frames and the game-over menu: the
clear comes before their early return 3, and before the slow-mode skip.

It is **not** issued on:
- loading calcs;
- the stage-end early returns (11 §4 step 1);
- the attract-demo end frames (return 3 after the fade);
- frames skipped by replay "slowdown reproduction" (playback speed byte 1).

The colour + depth clear of the playfield is a separate call in `Stage_DrawLow` (draw prio 6).

## 5. Demos

### 5.1 Demo 1: `stg4abg` background vertices — VERIFIED (code + draw log)

The quads with u 0.96875…1 (sprite 1 = 16 × 512 at (496, 0), `stg4abg.anm` script 1, tag 2) are **STD ribbons**
(quad type 1, 06 §11.3), about 68 per frame. Positions follow 06 §11.3. The half width is `|project(P + right·W) −
project(P)| / 2` with the **3D** length (x, y and depth). The vertex z is the projected depth, rhw is 1, and the batch
is drawn with fog off.

The colours explain the oddities in the log:
- End colour by distance d from the eye: VM colour 1 (+0x1F0) if d ≤ fog_near; fogged as in §11.1 if f < 1.
- If **f ≥ 1, only the alpha byte is cleared and the RGB keeps whatever the vertex slot held**. The 4 vertex
  colours are one stack array reused for all ribbons of the `Stage_DrawLayer` call.
  - A fully fogged A end (v0/v1) therefore shows the previous ribbon's A colours with alpha 0, e.g. 00515151.
  - Before any ribbon of the call wrote them, the slots hold stack residue: v0 = 0x00910D20, v1 = 0x00676E85, and
    B (v2/v3) = 0x00000000. The values are identical in every frame of demo 1 (62 262 + 57 338 of the A ends in the
    two chunks checked). Treat them as constants (INFERRED).
- Colours are computed before the direction / depth checks, so a ribbon skipped by those checks still updates the
  stale slots.
- In practice most ribbons have A alpha 0 and B alpha 0x00–0x60. With alpha test ≥ 4, only the part near B is
  visible, but the stale A RGB is interpolated into it.

For the draw-log diff, the RGB of alpha-0 vertices can be ignored if the port does not reproduce the residue.

### 5.2 Demo 1: night clock 129 frames early — VERIFIED (code + draw logs)

`times.anm` script 0 runs in HUD VM impl+0x34D4. `Hud_Update` runs that VM only when:
- not spell practice, and
- the **alpha byte of stage-title VM 0** (impl+0x2A44 + 0x1F3, `stgNtxt.anm` script 0) is ≠ 0.

The VM is drawn every frame, but it is hidden (alpha 0) until its own time 240.

Draw-log check:

| demo | title VM 0 fade-in starts | clock first drawn |
|---|---|---|
| 1 (stg4atxt script 0: alpha_time at 130) | frame 130 | **369** (= 130 + 239) |
| 0 (stg5txt script 0: alpha_time at 730; the other title VMs draw from 200) | frame 730 | **969** |

The port starts the clock VM at stage start, which gives 240 instead of 369: the 129-frame offset.

When the title fades out (alpha 0 at 4A frame 548), the clock VM stops ticking. In stage 4A its own fade-out has just
finished then (last draw frame 547, alpha 4).

### 5.3 The 2-digit text at the bottom right of the playfield — VERIFIED (code + draw log + replay data)

This is the **replay playback fps**, drawn by `Fps_Update` 0x446F53 (details now in 15 §3.3):
- Condition: replay playback (GameFlags bit 3) and gameplay running (bit 2).
- `Ascii_QueueString((384, 448), buffer 0x17CF2B8)`, format **"%2d"** (0x4B692C).
- Colour 0xFFFFFFD0 (pale yellow; the "D0" is the blue byte, not alpha), or 0xFFFF4040 when the recorded flag bit
  is set. The queue colour is reset to white afterwards.
- Glyphs are `ascii.anm#0` 16 × 16, advance 13 → (383.5, 447.5) and (396.5, 447.5).
- The buffer is re-formatted from fps_now only when the real-time fps meter closes a ≥ 500 ms window (host clock),
  so about every 30 frames with an arbitrary phase.
- fps_now itself comes from the replay's fps stream: one byte per 30 frames, read with the q+1 quirk.

Demo 1:
- fps bytes 60, 16, 60, 60, …;
- frames 1–4 show "60" (the previous demo's text);
- frames 5–34 show "16";
- from frame 35 on, "60".

A port should refresh the text every 30 frames from the replay byte. The exact refresh frame is host-timing
dependent in the original.
