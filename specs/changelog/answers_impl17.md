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

---

# Answers to IMPL17_QUESTIONS — menu part (Q1, Q3, Q4, Q6, Q7) (dirty room, RE-Q17M, 2026-10-19)

Sources: th08.exe (Ghidra dump), `specs/data/drawlog/menu_mopup1.*`, and probe runs of the mopup1 input script with peeks and
screenshots (`re/work/runs/q17m/`).

Spec sections changed:
- NEW `specs/data/spell_tables.md`: all spell-card tables and strings (difficulty lists, Last list, stage lists, difficulty and shot
  names, full-width digits, the 18 unlock hints, info-panel formats).
- 12 §1: result-viewer surface hand-over to slot 8.
- 12 §2.1: interrupt delivery rule (one pending slot, catch-all labels, flag bit 1).
- 12 §2.2: menu register cursor, description VM during the 9 render frames, the description VM on other screens.
- 12 §2.4: portrait/description VM enable bits.
- 12 §2.5: practice-screen entry (cursor, portraits, description).
- 15 §2.8: when `LastWord_ComputeUnlocks` runs; what it does to FLSP.
- 16 §2.4: result-viewer exit timing.
- 16 §6.1–§6.4: entry interrupts of screens 16/17, VM 141 reset, the row pen x (A.x − 32), the description VM.
- 10 §6: the font the reference runs actually use.

## Q1. Spell-card tables, LastWord_ComputeUnlocks and FLSP — VERIFIED

**Tables.** The full contents are in `specs/data/spell_tables.md`:
- 0x4C6B90: the 43-id "Last" list.
- 0x4C8628: six difficulty names, `初月（イージー）` … `極意（ラストワード）`, indexed by `Spell_DifficultyOf`. Shot names are a
  separate table, 0x4C7F4C.
- 0x4C82C8: the hint records. Each record is two lines, and each line is {format, 5 ints}; ints are printed + 1.
- The info-panel format strings.

Info-panel line 1 is `使用者  %s　　%s %s`. Its arguments, in order:
1. the owner (CATK +0x40), or `？？？？？？？？` if never attempted;
2. difficulty name[`Spell_DifficultyOf(id)`];
3. `Last` if the id is in the Last list, else `" "`.

**LastWord_ComputeUnlocks (0x46CBBB).**

When it runs:
- in `MainMenu_OnRegister`, on every menu load, right after the score.dat chunks are read;
- on every entry to screen 17.

What it does:
- It evaluates the 17 conditions of 15 §2.8.
- When a condition holds, it **writes** slot k (GM+0x18+k) = 0xCD + k.
- It never clears a slot: unlocks are monotonic. A slot set from a stale or edited file stays set.

FLSP life cycle:
- **Load.** The block (GM+0xC, 0x20 bytes, header included) is filled from score.dat only by `MainMenu_OnRegister` and the
  boot loader (`ScoreDat_GetFLSP`: chunk "FLSP" version 1, copied whole). When the file has no such chunk, the memory is
  left as it was: zero at boot.
- **Save.** Every `ScoreDat_Save` rewrites the header (magic, size 0x20/0x20, version 1) and writes the 0x20 bytes. Unlocks
  computed in the menu therefore reach the disk at the next save; starting any game saves (mode 2 flush, 15 §2.2).

`Card_IsSelectable(id)` = (id < 205 and (story attempts[12] or practice attempts[12] ≠ 0)) **or** byte GM+(id − 181) == id.

For 205–221 that byte is the FLSP slot. For lower ids the test reads unrelated bytes: GM+0..+0x17, or memory before GM.
- The only caller that passes such ids is the info-panel hint check, with id 204: it reads FLSP header padding, which is 0.
- So in practice the test means "attempted, or the Last Word slot is set".

## Q3. Menu interrupts — VERIFIED (code + title01 scripts + mopup1 log)

**Delivery rules.** These explain every case below.
1. A VM has **one** pending-interrupt slot. `AnmManager_InterruptRange` writes it for all 142 VMs. A later per-VM write in
   the same handler call overwrites it, so that VM never sees the range interrupt.
2. A VM whose script has no label for the id jumps to its catch-all label (−1, near the top of every title01 script). That
   label is followed by `hide_wait_interrupt` (hidden) or, in VM 141, by `pos 640,320` and then the hide.
3. Flag bit 1 ("enabled", 05 §2.1) is required for drawing, and the scripts never touch it. The menu handlers clear it to
   hide a VM **at once**, while its script still runs the fade label.

**Portraits / description on screens 9→11 and 15→16 (character VMs 111–130).**

Screen 11 entry and screen 16 entry (and every shot change on screen 16) do the same thing:
- range interrupt (18 on screen 11; 18 on screen 16);
- then for every VM 111–130: clear bit 1, set pending 8;
- then for the current shot's table row (0x4B8CD8, 4 ints):
  - entries 1 and 2: set bit 1, pending 9;
  - entry 3: set bit 1, pending 23.

So:
- The portraits never see 18. They get 9: label 9 = alpha 8 frames → 255, slide to (320, 240). Entry 3 gets 23 (dimmed partner).
- Entry **0, the character description** `sl_pltxt` (VMs 119–130), stays **disabled**. Its label 8 fade still runs, but it is
  not drawn from the entry frame on. This is the "description hides at once".

Character select (screens 5/9/13/15) differs:
- On entry, entries **0..2** get bit 1 + 9, so the description is shown.
- On a cursor move (sub-state 1), all VMs 111–130 get bit 1 **set** + 8, so the old portraits and description fade out
  visibly. Then entries 0..2 → 9 and entry 3 → 23.

**Headings 138 / 139 (screen 16).** After the range interrupt 18, the screen-16 entry block writes pending **27** into
VMs 138 and 139 (overriding 18).
- VM 138 (sprite 154) has label 27: alpha_fade 255 over 16 frames and a 30-frame slide to (320, 48). It fades in at once.
- VM 139 (sprite 155) has no label 27 → catch-all → hidden.

On screen 11 (plain range 18): 139 has label 18 → it fades in; 138 has no 18 → hidden.

**Interrupt 26 (screen 17).** Screen 17 entry sends range interrupt **26** to all 142 VMs. Labels 26 exist in:
- the portraits 111–118: alpha → 128 over 16 frames, back to (320, 240) (the chosen team stays half-visible behind the list);
- headings 138/139 (fade out);
- 140 (fade in).

Everything else, VM 141 included, goes to its catch-all.

**VM 141 restarting from (640, 320).** In script 141 the catch-all label comes **before** `pos 640,320`. Any interrupt
other than 18 or 6 resets the anchor to (640, 320) and hides it: 26 on screen 17, 5 when leaving the top menu, etc. Every
entry to screen 11 or 16 (interrupt 18) therefore slides it 640 → 256 again over 20 frames.

**32 px.** `MainMenu_SpellPracticeCharDraw` computes the heading position (A.x − 32, A.y − 152), stores it into VM E7B8, and
then **keeps using the same x variable as the text pen**. So:
- the rows (`"%s%s"`) are at x = A.x − 32;
- the numbers are at A.x − 32 + 182 = A.x + 150;
- the Total line is at the same x.

Steady state: rows x 224, numbers x 406 (the draw log has glyph quads at 236.5 for the 2nd character of a row, after the
1-char marker). 16 §6.4 said "x = A.x"; fixed.

**Practice cursor (screen 11).** Character-select confirm on screen 9 sets cursor := **g_StageIdx** (and screen 15 → 16 does
the same), then switches screens. Screen 11 never sets the cursor on entry. In sub-state 1, if bit cursor of the available
mask is clear, cursor := 0. So the cursor starts on the stage index of the last game, or on the spell-practice row of the
last spell-practice visit: screen 16's confirm and cancel both write g_StageIdx := row. In mopup1 that was row 1, which was
not available there → 0.

## Q4. Return from the result viewer / music room — VERIFIED (code + mopup1 log + probe peeks)

**Result viewer exit.** In state 1 the second Bomb/X (cursor already on 3) or a confirm on item 3 calls
`Result_SetState(2)`. The frame counter +4 is zeroed, then incremented at the end of that calc.

| frame (P = the X frame) | counter | event |
|---|---|---|
| P | 0 → 1 | state 2 |
| P+1 | 1 | `Loading_Begin((500, 440))` (nowloading VMs start, Supervisor+0x2FC = 1); all 72 result VMs interrupt 2 |
| P+2 … P+19 | 2 … 19 | — |
| P+20 | 20 (> 19) | next scene := 1, `Result_Calc` returns 0 → the object is removed: `Result_OnRemove` saves score.dat and **moves surface slot 0 (`result.jpg`) into surface slot 8** (0x45A3FD: release 8, move both surface pointers and the size record, clear slot 0) |
| P+21 | — | scene 1: `MainMenu_OnRegister` |

mopup1 check: X at present 2813, nowloading VMs first logged 2814, scene 1 at 2834 = P + 21. A probe peek shows slot 8's
surfaces appearing exactly in frame P+20.

The nowloading VMs are drawn in scene 5 too (prio 2), but the result screen's own full-screen `CopyRects` of `result.jpg`
(Result draw, prio 18) covers them. They become visible on the first scene-1 frame, already at script time 20.

Without the capture flag, `Supervisor_DrawLoading` shows slot 8 = the plain `result.jpg`: no VMs and no text. The probe
labels it `K th08logo.jpg` (slot name). A screenshot at present 2835 shows the result background with "Result Stage" and
the loading banner, and no ranking.

**Menu register.** `MainMenu_OnRegister` (prev scene = 0x17CE8B8):

| prev scene | loading call | cursor |
|---|---|---|
| 0 (boot) | none (the boot loading screen is still running) | 0 |
| 1 | `Loading_Begin` | 0 |
| 2 (game) | `Loading_BeginCapture` + darken overlay (12 §1) | 1 if the last difficulty (0x160F538) was Extra (> 3), else 0 |
| 3, 6 | `Loading_Begin` | same as 2 |
| 5 (result viewer) | `Loading_Begin` | **5** |
| 8 (music room) | `Loading_Begin` | **6** |
| other | `Loading_Begin` | 0 |

Then:
- if GameFlags bit 0 (practice): cursor := 3, or 2 if bit 14 (spell practice); +0x6C := 1 / 2;
- flags &= ~0x4001.

`Loading_Begin` does nothing when the loading state is already ≠ 0. After the result viewer (state already 1 since P+1)
and after the music room (its exit calls `Loading_BeginCapture` on the exit frame), the banner animation simply continues.
That is why the loading animation is ~21 frames "ahead" compared with a game → menu return. The menu loader thread ends
with `Loading_End` (fade-out, interrupt 1) when its files are loaded: 32 frames in mopup1; loader-dependent, see the
"Loader threads" README.

**Description during the first 9 top-menu frames.** The loader thread's last step sets the current description VM
(+0xC2BC) = description VM **0** and starts all 14 description VMs on text.anm script 9. Script 9 starts directly with
"label 1, alpha 0, alpha_time 20 → 255", so VM 0 fades in by itself.

Top-menu sub-state 0 then spends 9 frames rendering one description text per frame (item k on frame k). On the 9th frame it
sets +0x08 (previous cursor) := −1 and enters sub-state 1 in the same call. Only then does the "cursor changed" test fire:
+0xC2BC := VM[cursor], interrupt 1 (restart the fade-in).

So for 9 frames the original shows item 0's description, whatever the cursor. mopup1, present 2866–2874: v 0–0.066 =
text strip 0, alpha 0x19 → 0x7F. From present 2875 it shows the cursor's text (strip 5, alpha from 0x0C). The same happens
at boot, present 46–54 then 55, where both are item 0. The cursor item highlight is correct from the first frame.

## Q6. Text rasterizer layout; 取得率 heading — VERIFIED (code) / measured (reference runs)

The game has no tab or column logic. Every text strip is one `vsprintf` string:
- columns come only from ASCII spaces, `%3d` padding and U+3000 full-width spaces in the format strings
  (`specs/data/spell_tables.md` §8);
- shot names are padded to 8 full-width cells.

The string is drawn with GDI `TextOut` at (2, 2) of a 2× work surface and box-downsampled (10 §6). Horizontal positions
therefore depend only on the font's advance widths.

The game asks for `CreateFontA(height 28, weight 600, charset SHIFTJIS, pitch FIXED|FF_ROMAN, "ＭＳ ゴシック")`. On
Windows that is MS Gothic, which is monospaced: half-width 7 px, full-width 14 px at 1×.

**The reference runs did not use MS Gothic.** A `WINEDEBUG=+font` run shows:
- The face name reaches wine garbled. The ANSI code page of the process is not 932, so the Shift-JIS bytes are converted
  as cp1251/1252.
- Wine falls back to **Droid Sans Fallback** (`/usr/share/fonts/droid/DroidSansFallbackFull.ttf`), ppem 21 for height 28,
  synthetic bold (weight 600 → 700).
- That font is **proportional**. Measured in `menu_ref/mopup1_sp17.png`: full-width `？` advance 11 px, digits 6.5 px,
  ASCII space much narrower.

So the info panel, the screen-16 headings and every strip in the `menu_ref` screenshots and session recordings show Droid
Sans Fallback layout. The ~20 px column shift on info-panel lines 2–3 (the `%3d` blanks and U+3000 runs) is this font
difference, not a layout rule.

Port choices:
- (a) a monospace 7/14 px font = the Windows original;
- (b) to match the reference screenshots, Droid Sans Fallback at 21 ppem with emboldening.

The draw logs are unaffected: they record the strip quads, not the texture contents.

**取得率 heading weight.** VM EA5C and the other heading VM E7B8 are rendered by the same call: `Text_RenderToVm`, white,
outline 0 (black ±2 px at 2×), font bytes 15/15 → the weight-600 path, scale 1. The only code difference is the ED00 quirk
(16 §6.4: ED00's init writes EA5C's flags and colour again), and it changes neither font nor pixels. Any visible weight
difference comes from the substitute font's emboldening on those particular glyphs. There is no separate rule to port:
use the same rasteriser settings for both headings.

## Q7. Spell practice: fading description; card-name VM — VERIFIED (code + mopup1 log)

**Fading description.** Top-menu confirm on "Spell Practice" sends interrupt 2 to the current description VM (item 2). The
VM stays current: +0xC2BC is only replaced on the top menu or option screens.

`MainMenu_Calc` runs it, and the menu draw draws it, on **every** screen. It is drawn after VMs 0–141 and before the
screen's list draw, at (320, 448) centred.

Script 9 label 2: alpha 255 → 0 over 60 frames, then `hide_wait_interrupt`. So it **stays drawn during screen 16** until
it reaches alpha 0, about 60 frames after the confirm. mopup1 present 400 (screen 16) still draws strip 2 at alpha 0x33.

Two details:
- Draw2DSnap skips alpha 0, so nothing is drawn after the fade.
- The description strips are `text.anm` sprites 21+k, and they are the **same texture rows as the screen-17 info VMs**
  (sprites 21–27). If screen 17 is entered while item 2's text is still fading (possible from screen-16 frame 9), its
  strip (sprite 23) is overwritten by info line 2. The fading VM then shows `取得数/挑戦数[最高点]…` at the bottom
  centre. The top menu re-renders all 9 descriptions on every entry, so this never persists.

**Card-name VM under the spell row.**
- Replay detail of a **spell** replay (12 §2.8): the scratch VM E7B8 (`resulttext` sprite 11, card name from the file
  header +0x7E) is drawn every frame of sub-states 2 and 3 at VM 97's pos + (0, 16), with `Draw2DSnap` (verified in the
  disassembly: push menu+0xE7B8 before the call). It is visible: `AnmVm_Init` → flags 7, and `Text_RenderToVm` sets bit 0.
  For normal replays it is rendered at selection but not drawn.
- Screen 17: all ≤ 15 row VMs (card names) are drawn every frame at (118, 94 + 16j). Rows that are not the cursor row use
  colour 0xFF606060. mopup1 present 480: 15 rows plus the header.

## Remaining uncertainties
- MS Gothic advances (7/14 px) are from the font's design (monospace, em = cell height 28 at 2×). They are not measured on
  Windows (INFERRED).
- The menu loader's duration (Loading_End frame) depends on the loader race. Use the session's `release` events.
