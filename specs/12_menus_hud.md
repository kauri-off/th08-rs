# 12 — Boot logo, main menu, in-game menus, HUD

Sources: th08.exe via Ghidra (names in `symbols.csv`), `title01.anm`, `front.anm`, `text.anm`,
`nowloading.anm`, `ascii.anm` (dumps `extracted/anm_dis.txt`, `anm_index.txt`). VERIFIED = read from code/data;
INFERRED = interpretation. "Pressed" = bit set in the raw input word 0x164D528 and not in the previous raw word
0x164D530. Input bits (11 §7 / 10 §5): SHOT 0x1, BOMB 0x2, ESC 0x8, UP 0x10, DOWN 0x20, LEFT 0x40,
RIGHT 0x80, SKIP (Ctrl) 0x100, Q 0x200, S 0x400, Home/P 0x800, ENTER 0x1000, D 0x2000, R 0x4000 (keyboard map
0x43D970, VERIFIED changelog/answers_phase5.md). "Confirm" = SHOT|ENTER (0x1001), "Cancel" = BOMB|ESC (0x0A). Key repeat flag
0x164D538 (Supervisor poll, exact rule in 11 §7): set on the 33rd, 41st, 49th, … consecutive frame with an unchanged
raw input word (the counter is tested before it is incremented).

Common sounds: 10 `se_ok00` confirm, 11 `se_cancel00` cancel, 12 `se_select00` cursor move.

## 1. Boot, logo and loading screens — VERIFIED

Supervisor on-register (0x445FA3): load `score.dat`, compute unlock flags (game flags bits 15–17), open the
archive and version file, then **load `title/th08logo.jpg` into full-screen surface slot 8**, load
`nowloading.anm` (ANM slot 2), and enter the "loading screen" state at screen position (500, 440)
(`Loading_Begin` 0x448790); the remaining start-up (RNG seed, `text.anm`, sound, …; `Supervisor_Init`
0x4464C8) runs on a loader thread.

Loading-screen state = Supervisor+0x2FC:

| state | meaning | drawn (Supervisor draw prio 0, 0x445BD4) |
|---|---|---|
| 0 | none | surface 8 released if loaded |
| 1 | loading | surface slot 8 drawn full screen (logo at boot); three VMs run `nowloading.anm` scripts 0, 1, 2 at the given position (draw prio 2, 0x445D3E): "now loading" pulse, text sliding in, rotating ring |
| 2 | loaded, waiting (`Loading_Wait` 0x448972, interrupt 2 to the VMs) | as 1 plus, from the 5th frame, the ASCII text "Press Shot Button" at (288, 454), scale 0.5, alpha pulsing (frames 5–34: 255 − (t−5)·128/30, then rising back, period loop at 64 → 5) |
| → 0 | `Loading_End` 0x44881A: interrupt 1 to the VMs (fade out), state 0 | |

- Boot: the logo stays visible until the main-menu loader thread (0x470E10) has loaded `title01.anm` /
  `resulttext.anm` / `title00.png` and calls `Loading_End`; then the menu takes over. (00 §3.2 for title BGM.)
- Stage loading (GM on-register 0x43AAF4): when the previous scene was the main menu (1), `Loading_Begin` at
  (500, 440) and the full-screen surface-8 view is enabled; otherwise (next stage, retry) `Loading_Begin` at
  (280, 430). The loader thread calls `Loading_Wait` after the stage is ready; after a stage clear it
  waits there until the player confirms (11 §4).
- **Boot presents** (changelog/answers_impl15.md §Q1, VERIFIED runtime): presents 1–2 = `Window_InitD3D` (0x45B020: clear black +
  Present, twice); from present 3 `Frame_Run`. Presents 3–5 contain no primitives (cause INFERRED: loader-thread
  start-up), but the nowloading VMs already tick every calc; present 6 is the first logo frame (VMs at script time 5).
- Loading VM draw 0x445D3E (prio 2, VERIFIED): for each of the 3 VMs pos += pos_offset, `Draw2DRot`, pos −= pos_offset.
- `0x44886F(pos)` = `Loading_Begin(pos)` + post-present capture request (AnmManager+8 = 8, rect 0,0,640,480, only if no
  capture is pending): after the next Present the back buffer is copied into surface slot 8, which
  `Supervisor_DrawLoading` then shows. Used by `GameMgr_OnRegister` (both branches), the main-menu loading paths and
  the music-room exit; the probe labels this `K screen th08logo.jpg` (slot name), but it is the captured last frame.
- **Result viewer → menu** (changelog/answers_impl17.md §Q4, VERIFIED): `Result_OnRemove` moves surface slot 0 (`result.jpg`) into slot 8
  (0x45A3FD: release slot 8, move both surface pointers and the size record, clear slot 0). The loading screen of the
  following menu load shows the plain `result.jpg` (no capture). The result viewer already started the loading state, so
  the menu's `Loading_Begin` is a no-op and the banner animation continues (it started at result frame P+1).
- Stage start from the main menu also creates the loading darken overlay (ScreenEffect type 5, 13 §3): full-screen
  black, alpha 0 while loading, 0x80 → 0x00 over game frames 1–9, drawn at draw prio 1 (first in the frame).

## 2. Main menu (`MainMenu_Register` 0x47146D) — VERIFIED

Object 0x14490 bytes (global 0x18BDE08). Calc prio 4 (0x467399), draw prio 3 (0x47087F).

| off | meaning |
|---|---|
| +0x00 | cursor |
| +0x08 | previous cursor (for change detection) |
| +0x0C | sub-state of the current screen (0 enter/animate, 1 interactive, 2/3 leaving) |
| +0x10 | frames in sub-state |
| +0x68 | previous screen |
| +0x6C | 1 = return to practice, 2 = return to spell practice (set on re-entry from a practice game) |
| +0xC2B0 | `title01.anm` file |
| +0xC2B8 | VM array (142 VMs; VM *i* runs `title01.anm` script *i*) |
| +0xC2BC | current item-description VM |
| +0xC2C0 + i·0x2A4 | 9 description-text VMs |
| +0x14424 | VM count (0x8E) |
| +0x14428 | **screen** |
| +0x1442C | frames on this screen |
| +0x14430 | idle frames (demo timer) |
| +0x14434 | quit state (2 → return 4) |

Screen dispatch (0x467399):

| screen | function | purpose |
|---|---|---|
| 0 | 0x4674E0 | title / top menu |
| 2 | 0x46837B | option |
| 3 | 0x469636 | key configuration (returns to 2) |
| 4, 8, 12 | 0x46A071 | difficulty select (4 game, 8 practice, 12 Extra) |
| 5, 9, 13, 15 | 0x46A625 | character select (5 game, 9 practice, 13 Extra, 15 spell practice) |
| 7 | 0x46E136 | replay select |
| 11 | 0x46ADB0 | practice stage select |
| 16 | 0x46B174 | spell practice: character/stage |
| 17 | 0x46BBC0 | spell practice: card list |

After the screen function, all 142 VMs are run every frame (and the description VM). A screen switch
(`Menu_SetScreen` 0x4694C3) resets cursor timers/sub-state and stores the previous screen.

**Screen entry rule (changelog/answers_impl15.md §Q12, VERIFIED).** `Menu_SetScreen` (0x4694C3) runs inside the old handler (which
then returns) and zeroes +0x10, +0x1442C, +0x0C, +0xC294. The new handler first runs on the next calc frame; all nine
handlers begin with an entry block guarded by +0x1442C == 0 (range interrupts, cursor sprites/interrupts), and
`AnmManager_RunRange` runs after the handler in the same `MainMenu_Calc`, so a press on frame P gives the new screen's
interrupts and their VM effect on frame P + 1.
**Exception — handlers that return 2** (RE-Q25, VERIFIED return values of every handler): `MainMenu_Calc` returns the
handler's value, and calc return 2 makes the chain call the same node again at once (00 §3). These switches therefore
run the new screen's handler **in the same frame**, and `RunRange` + the description VM run **twice** in that frame
(once after each handler):
- character select (9/15) **confirm** → 11 (practice) / 16 (spell practice): `Menu_SetScreen` then return 2;
- practice stage select (11) **cancel** → 9: return 2;
- spell-practice character screen (16) → 17, both on confirm and on the +0x6C forward (below): return 2;
- card list (17) **cancel** → 16: return 2.
Every other switch (all difficulty-screen transitions, the character screen's cancel and its +0x6C forward, the top
menu, replay, option) returns 1 and follows the next-frame rule. This is why the practice stage screen and the 16 ↔ 17
switches show the new screen on the press frame in the draw logs.
**Delayed switches and input start (VERIFIED, RE-Q25).** Every handler increments the sub-state counter +0x10 and the
screen counter +0x1442C at the end of each call; a press zeroes +0x10 in its own call. A test "+0x10 > 19" (> 29,
> 59) is therefore true in the call P + 20 (P + 30, P + 60) after the press on frame P, which calls `Menu_SetScreen`
(return 1), and the next screen first runs on P + 21 (P + 31). Uses: difficulty / spell-practice-char cancel 20,
character cancel 30, replay leave 30 (top cursor 4), option item 7 → 30, Quit 60. Input start: screens 4/8/12, 5/9/13/15,
11, 16, 17 switch to their input sub-state in the call where +0x1442C == 8 (input from that call on); the replay
screen leaves sub-state 0 when +0x1442C > 7 and accepts confirm/cancel when +0x10 > 9; the option screen moves the
cursor from its first call and accepts Left/Right/confirm/cancel from +0x1442C ≥ 4; the top menu from frame 10.

### 2.1 title01.anm VM map (script index = VM index) — VERIFIED from the anm dump

| VMs | content |
|---|---|
| 0 | background (`title02.png`) at x 384 |
| 1–9 | top-menu items 0–8 at (64, 180 + 28·i); each item has two sprites: **selected = the script's own sprite, unselected = sprite + 1** |
| 10–42 | option menu labels/values |
| 43–76 | key-config labels |
| 77–110 | replay list (77 header, 78–107 thirty rows, 108–110 headers) |
| 111–118 | character portraits (`sl_pl00h`, `sl_pl00a`, `sl_pl01a`, `sl_pl01h`, `sl_pl02h`, `sl_pl02a`, `sl_pl03a`, `sl_pl03h`) at (416, 240) |
| 119–130 | character description texts `sl_pltxt0..11` at (480, 352) |
| 131–134 | difficulty images (Easy…Lunatic, `select01.png`) |
| 135 | Extra difficulty image |
| 136–141 | headings (`sl_text.png`: e.g. select difficulty / character) |

**Interrupt delivery** (changelog/answers_impl17.md §Q3, VERIFIED):
- Each VM has one pending slot (+0x1FE). A per-VM write after `AnmManager_InterruptRange` in the same handler call replaces
  the range interrupt for that VM.
- A VM without a label for the id takes its catch-all label (−1). In every title01 script the catch-all is followed by
  `hide_wait_interrupt`. In script 141 it is followed by `pos 640,320`, so any interrupt other than 18/6 resets VM 141
  to (640, 320), hidden.
- Handlers hide VMs instantly by clearing flag bit 1 (enabled, 05 §2.1); the script's fade label still runs undrawn.

Screens drive the VMs with interrupts sent to the whole range (`AnmManager_InterruptRange`): 2 = show top menu,
5 = leave top menu to a sub-screen, 13 = to replay screen, 1 = quit/fade out, 7 = enter difficulty select, 12 =
enter Extra difficulty, 6 = cancel back, 8 = enter character select, 18 = practice stage select. Individual VMs
get: description VM 1 = show, 2 = hide; difficulty VMs 24 = selected, 25 = not selected; character VMs 9 = show,
8 = hide, 23 = main portrait.

### 2.2 Top menu (screen 0) — VERIFIED
Background (changelog/answers_impl18.md §Q3, VERIFIED code + draw logs): surface 0 (`title00.png`) is copied by the menu's own
draw (prio 3, 0x47087F: `Anm_DrawSurface(0)` first, then the 142 VMs and the description VM). The nowloading VMs of
`Supervisor_DrawLoadingVms` (prio 2) are queued earlier in the sprite batch, but `Anm_DrawSurface` is an immediate
`CopyRects` that does **not** flush the batch, so the device receives the copy first and the batched nowloading
quads after it (flushed by the next state change, i.e. the first title01 quad): the nowloading sprites end up **on
top of** the background. A port must draw surface 0 before the prio-2 loading VMs (or emulate the deferred batch).
Entering: request title BGM slot 8 if coming from a non-menu scene; load surface 0 = `title/title00.png` when
coming from the title/difficulty/replay screens; create the 142 VMs once; interrupt 2. If a replay was being
watched → go straight to the replay screen. If `+0x6C` ≠ 0 (back from a practice game; 2 =
spell practice, which also sets flags bit 14) → screen 8 + 7·(flags bit 14), i.e. **8** (practice difficulty) or **15**
(character select, spell-practice variant), range interrupt 5. Each of these screens forwards at once while +0x6C ≠ 0
(VERIFIED code; `musicroom_practice` trace shows menu 0 → 15 → 17 on consecutive frames 7887–7889):
practice 8 → 9 (cursor 0) → 11 (cursor = g_StageIdx), one screen per frame (each forward returns 1); spell practice
15 → 16 (cursor = g_StageIdx, return 1) on one frame, then 16 → 17 (cursor 0; g_StageIdx := 9 if the last card id
> 204) **and** screen 17's first call on the next frame (16 returns 2, see the screen-entry exception above). Only
screens 11 and 17 clear +0x6C (in their entry blocks). Then for 9 frames render one description per frame into the 9 description VMs
(`Text_RenderCentered` 0x466650: centred in the sprite, text colour 0xFFF0E0, 2-px black outline, font 15).
The 14 description VMs use text.anm script 9 (sprite 21 + i, centred at (320, 448); interrupt 1 = fade in 20
frames, interrupt 2 = fade out 60); only the current one (+0xC2BC) is run/drawn (changelog/answers_impl9.md §15):

| item | text | action on confirm |
|---|---|---|
| 0 Start | ゲームを開始します | clear practice flags; cursor = last difficulty (config +0x21, > 3 → 2; a dead store: the difficulty screen's entry block overwrites it next frame with its own > 3 → 1 rule, §2.3); screen 4 |
| 1 Extra Start | エキストラステージを開始します | only if Extra is unlocked for any team; screen 12 |
| 2 Spell Practice | 敵にスペルカード戦をお願いします | only if spell practice unlocked: flags |= 0x4001; cursor = shot; screen 16. If locked: behaves as Replay |
| 3 Practice Start | ステージを選択し、練習を開始します | flags |= 1; cursor = last difficulty; screen 8 |
| 4 Replay | リプレイを鑑賞できます | screen 7 (interrupt 13) |
| 5 Result | 過去のスコアやスペルカードの取得歴を見られます | scene 5 |
| 6 Music Room | 音楽を聴けます | scene 8 |
| 7 Option | 各種設定できます | after 30 frames: screen 2, config snapshot saved |
| 8 Quit | いろいろと終了します | range interrupt 1, sub-state 2; **no music change in WAV mode** (MIDI mode only: `Midi_Stop`, `Midi_Open(…, 30)`, `Midi_Play`; there is no fade, VERIFIED 0x4674E0, RE-Q25); in the call 60 frames later free VMs, scene −1 |

- Cursor (`Menu_CursorUpDown` 0x47030B, wraps, sfx 12; held keys repeat with the repeat flag); locked items 1
  and 2 are skipped and drawn with colour 0xFF404040.
- Every frame: all item VMs get the unselected sprite, the cursor item its base sprite; when the cursor
  changes the new description VM gets interrupt 1.
- **Description VM timing** (changelog/answers_impl17.md §Q4, VERIFIED, mopup1 presents 2866–2875):
  - The menu loader ends by setting +0xC2BC = description VM 0. All 14 description VMs start text.anm script 9, which
    fades in by itself (alpha 255·t/20).
  - Release frame (changelog/answers_impl18.md §Q3, VERIFIED code + selftest1 presents 46–48): the loader tail sets +0xC2BC,
    +0x14434 = 0 and `Loading_End` before that frame's calc (the session park point releases it at Frame_Run
    pre-calc), so `MainMenu_Calc` runs fully in the release frame: the Top handler's entry block (interrupt 2 to
    the item VMs), `RunRange` and `AnmVm_Run(+0xC2BC)`. The description VM therefore ticks for the first time in the
    release frame, exactly like the title01 VMs. Nothing menu-side is drawn in that frame (the loading screen still
    shows); the first drawn frame has t = 2 for both: description alpha 0x19 (= 255·2/20), title01 VMs 0x1E.
    No interrupt 1 is involved before sub-state 1.
  - During the 9 render frames of sub-state 0 the drawn description is therefore item 0, whatever the cursor.
  - On the 9th frame the handler sets +0x08 = −1 and enters sub-state 1 in the same call. The change test then switches
    to VM[cursor] with interrupt 1 (fade-in from 0).
- **Cursor on menu load** (`MainMenu_OnRegister`, by previous scene):
  - 5 (result viewer) → 5;
  - 8 (music room) → 6;
  - 2/3/6 (game) → 1 if the last difficulty was Extra, else 0;
  - otherwise 0;
  - then practice flags override: practice → 3, spell practice → 2, with +0x6C = 1/2.
- **The current description VM is run and drawn on every screen** (menu calc/draw, after VMs 0–141), at (320, 448).
  Leaving the top menu sends it interrupt 2 (fade 60 frames, then hidden), so it stays visible for ~60 frames on the next
  screen.
  Exact senders (VERIFIED `MainMenu_Top` 0x4674E0, RE-Q24): the pending interrupt 2 is written into the current description VM (pointer menu+0xC2BC; it is **not** in the title01 range, so range interrupts never reach it) by every confirm that leaves to another screen or scene — Start/Extra/Practice/Spell Practice (with their range interrupt 5/7/…), Replay (range 13), Result viewer (scene 5), Music Room (scene 8) — and by the entry redirects (returning to the replay screen after a replay, range 13; returning to practice / spell practice, range 5). **Option (item 7) and Quit (item 8) send nothing to it**: Option sets sub-state 3 and calls `MainMenu_Option` 0x46837B in place, Quit (sub-state 2) only sends range interrupt 1, so on those the current description keeps its state.
  - Description strips are text.anm sprites 21+k = the same rows as the screen-17 info VMs. Screen 17 entry overwrites
    strips 0–6.
  - The top menu re-renders all 9 descriptions on each entry.
- Input accepted from frame 10 on. Confirm: sfx 10 (+ sound flush). Cancel: cursor jumps to Quit (sfx 11).
- **Demo**: idle counter +1 per frame, reset by any raw input; after 1500 frames load
  `demo/demorpy((n+1) mod 4).rpy`; on success: flags = replay | demo (bits 3, 1), difficulty/shot from the
  replay header (+0x6B / +0x6A), byte 0x164D0B2 = shot % 2 (changelog/answers_phase7.md §3), stage = first stage with a record, scene 2. (Demo exit: 11 §4.)

**What the top menu draws** (menu draw 0x47087F, every frame while quit state +0x14434 = 0; VERIFIED, changelog/answers_bugs2.md
§C1; ref `c1_menu_practice_sel.png`):
1. **Background.** Surface slot 0 (`title00.png`) is copied to the back buffer with `IDirect3DDevice8::CopyRects`
   (`Anm_DrawSurface` 0x466BB0: full image rect to dest point (0, 0)). It is a plain pixel copy: no quad, no
   filtering, no blending, no seams.
2. **VMs 0..141 in index order**, then the description VM. For each VM whose texture is loaded: pos += pos_offset
   (+0x288) temporarily, then `Draw2DSnap` if rotation z = 0 else `Draw2DRot`. Items are one quad each:
   - top-left anchored, 32 px tall, 28 px apart, so each item overlaps the next by **4 px**;
   - later items draw over earlier ones;
   - texture `title01.png` 512×512 A4R4G4B4;
   - normal alpha blend, alpha test ≥ 4, z-write off, LINEAR filter, WRAP addressing (05 §12.4);
   - colour white (locked items 0xFF404040).

   Example: Practice Start = VM 4, selected sprite 5 (atlas x 240 y 0, 192×32) / unselected 6 (240, 64), pos (64,
   264). Replay = VM 5, sprites 7/8 (0, 32 / 0, 96, 96×32), y 292. Item x positions wobble slightly
   (`pos_time_out2` loops); only Practice Start stays at x 64.
3. The items' white glow (alpha 17–68) runs to the sprite's edge rows and continues in the neighbouring atlas
   sprites. For example, atlas rows 31 and 32 at x 240–432 both carry glow. In the original the glow just ends at
   the quad edge, because snapped 1:1 sampling never reads row 32 (05 §12.4 consequence). A visible line at an item
   edge in a port means the neighbour row is being filtered in.

### 2.3 Difficulty select (screens 4/8/12) — VERIFIED
Enter: load surface 0 = `title/select00.png` (unless coming back from character select), interrupt 7 (12 for
Extra; Extra always selects the single Extra entry, VM 135). Cursor = config +0x21 (last difficulty; > 3 → 1), set in the entry block (this is the value that counts; the top
menu's > 3 → 2 store is overwritten, RE-Q25). Extra (12): cursor = +0x21 − 4 (≥ 0).
Interactive from frame 8: up/down over 4 entries (VMs 131–134; selected gets interrupt 24 and its base sprite,
others 25 and sprite+1). Confirm → store config +0x21 (Extra: 4 + cursor), sfx 10, screen 5 / 9 (practice) /
13 (Extra). Cancel → store, sfx 11, interrupt 6, after 20 frames back to screen 0 (cursor 0 / 3 practice /
1 Extra), flags &= ~1.

### 2.4 Character select (screens 5/9/13/15) — VERIFIED
Choices = 4 teams, or 12 (teams + 8 solos) when Extra is unlocked for all four teams (0x447D04). Extra
(13) and spell practice (15) skip characters that are not unlocked for them. Left/right move (wrap, sfx 12;
`Menu_CursorLeftRight` 0x470424). Shown VMs per character from table 0x4B8CD8 (4 ints per shot):

| shot | VMs (main portrait, extra portraits…) |
|---|---|
| 0 | 119, 111, 112 | 
| 1 | 120, 114, 113 |
| 2 | 121, 115, 116 |
| 3 | 122, 118, 117 |
| 4 | 123, 111 (−1, 112) |
| 5 | 124, 112 (−1, 111) |
| 6 | 125, 114 (−1, 113) |
| 7 | 126, 113 (−1, 114) |
| 8 | 127, 115 (−1, 116) |
| 9 | 128, 116 (−1, 115) |
| 10 | 129, 118 (−1, 117) |
| 11 | 130, 117 (−1, 118) |

(Enable/interrupt details, changelog/answers_impl17.md §Q3, VERIFIED. Here "enabled" = flag bit 1.)
- **Cursor** (changelog/answers_impl18.md §Q3, VERIFIED code): the difficulty screen's confirm calls `Menu_SetScreen(5/9/13)` and
  then sets cursor := 0; the character screen's entry block (its first calc, +0x1442C == 0) sets cursor :=
  g_ShotType (0x164D0B1, the last used shot type) before any input handling. The session trace's `cursor` column is
  sampled at `MainMenu_Calc` **entry**, so on the screen's first row it shows the 0 left by the difficulty confirm
  (selftest1 row 440; shot type 0 there, so the two cannot be told apart).
- **On entry:** every VM 111–130 is disabled and gets 8; table entries 0–2 are enabled and get 9; entry 3 is enabled and
  gets 23.
- **On a cursor move:** every VM 111–130 is enabled (not disabled) and gets 8, so the old ones fade out visibly; then
  entries 0–2 → 9 and entry 3 → 23.
- **Screens 11 and 16** (entry, and screen 16's shot change) use entries **1–3 only**. Entry 0, the `sl_pltxt`
  description, stays disabled and disappears at once. The portraits get 9/23, which replaces the range interrupt 18.
Confirm: shot = cursor, sfx 10; practice → screen 11; spell practice → screen 16; else
`difficulty = config+0x21`, `stage = 0` (Extra: `difficulty + 4` = 8), flags &= ~replay, stop music,
**scene 2** (new game). Cancel: back to the difficulty screen (spell practice: fade back to the top menu).

**Difficulty plate** (VERIFIED, changelog/answers_bugs2.md §C2; ref `c2_charsel_team0_normal_*.png`). On the first frame of
screens 5/9/13 (not 15), right after the range interrupt 8, VM `131 + config+0x21` (Easy 131 … Lunatic 134;
Extra 135 since config+0x21 = 4 there) gets interrupt **9**. That overrides the 8 the same frame (one pending slot),
so this one VM does not hide. Script label 9 does `pos_time_out2` → (16, 384) over 20 frames (top-left anchored)
and keeps the alpha and sprite from the difficulty screen. The sprite stays the base ("selected") sprite: 134/136/
138/140/142 for E/N/H/L/Ex, 256×80, `select01.png`. It includes the moon, the "… Level" text and the description
line. Label 18 (practice stage select) pins it at (16, 384). Labels 6/8/10 slide it back and fade it out when leaving.

**Final-route notice** (VERIFIED, `MainMenu_CharacterDraw` 0x47052D, called from the menu draw 0x47087F only on
**screen 5** = normal game, not practice/Extra/spell practice). It is drawn only while the screen's frame counter
(+0x1442C) is > 8. It uses the **cursor** team c (menu +0x00, not `g_ShotType`) and d = config+0x21. With CLRD
(15 §2.4) nc = no-continue mask (+0x0C) and any = any-clear mask (+0x16), the sprite (`title01.anm`, select01.png,
144×16) is:

| condition (first match) | sprite | text |
|---|---|---|
| nc[d] & 0x80 and any[d] & 0x40 | 146 | ——FinalA,B クリア—— |
| nc[d] & 0x80 | 148 | ——FinalB　クリア—— |
| c < 4 and no nc[E..L] has 0x80: if some any[E..L] has 0x40 | 145 | ——FinalB 挑戦可能—— (none otherwise) |
| otherwise (another difficulty has nc & 0x80, or c ≥ 4) | 147 | ——Final　選択可能—— |

Sprite 149 (FinalA クリア) is never used. The notice is drawn with a scratch VM at menu +0xE7B8: `AnmVm_Init`, set
sprite, anchor top-left (flags |= 0x1800), colour 0xFFFFFFFF, pos (400, 170, 0), `Draw2DSnap`. It is drawn after
all 142 VMs and the description VM. Bits: 0x40 = stage index 6 (6A) cleared, 0x80 = stage index 7 (6B) cleared
(15 §2.4, `1 << stage_idx`).

### 2.5 Practice stage select (screen 11) — VERIFIED
Entry (frame 0):
- range interrupt 18 (VM 139 fades in; VM 138 has no label 18 → hidden; VM 141 slides 640 → 256);
- portraits/description as in §2.4 (description hidden at once);
- +0x6C = 0;
- practice flag set.

The cursor is **not** set on entry: it is g_StageIdx, copied by the character-select confirm. That is the stage of the
last game, or the last spell-practice row. Sub-state 1 resets it to 0 if that stage is unavailable.

Sets practice flag. Available stages = any-clear mask (`gm+0x3C4A6 + shot*0x24 + diff*2`) for the chosen
difficulty and shot (empty → stage 1 only; if the 6B bit is set, 4A and 4B are both added). Cursor over the 8
entries 0..7 (St1, St2, St3, 4A, 4B, St5, 6A, 6B), skipping unavailable ones. Confirm: difficulty, stage =
cursor, scene 2. Cancel: back to character select.

**Draw** (`MainMenu_PracticeStageDraw` 0x46F429, from the menu draw on screen 11; VERIFIED, ref
`menu_ref/mopup1_prac11.png`). All text goes through the ASCII string queue (§2.9). Anchor A = **current
position of VM 141** (raw `pos`, no pos_offset; title01 script 141 is an invisible sprite whose label 18 slides it
from (640, 320) to (256, 320) over 20 frames, label 6 slides it to (768, 320)), so the table slides in with it.
- Header `Stage    HI-Score` at (A.x, A.y − 96), colour white, no highlight.
- Mask m = any-clear mask CLRD[shot].any[cfg difficulty] (same as the logic above): 0 → 1; bit 7 set → |= 0x18.
- 8 rows r = 0..7 at (A.x, A.y − 80 + 16·r): `"%s %9d0 (%3d)"` with the stage name from table 0x4C8250
  (`Stage1 `, `Stage2 `, `Stage3 `, `Stage4A`, `Stage4B`, `Stage5 `, `Stage6A`, `Stage6B`, 7 chars), PSCR hiscore
  (÷10) and PSCR play count for [shot][stage r][difficulty] (15 §2.6).
- Row colour: cursor row = white **with highlight** (§2.9); other rows 0xFFA0A0A0 if bit r of m is set, else
  0xFF404040.
- Steady state: header y 224, rows y 240…352, x 256.

### 2.6 Spell practice (screens 16/17) — VERIFIED calls; full screen/selection details and draw layout in 16 §6 (VERIFIED)
Screen 16 chooses character and stage (stage 9 used for last words); screen 17 lists the cards of that stage
from table 0x4C6C40[stage] (spell ids, i16); the chosen id is stored in 0x164D0B8, difficulty derived from the
id (0x4144D0, Normal for some), stage index derived from the id for last words; then scene 2.

### 2.7 Option (screen 2) — VERIFIED (item order and labels confirmed on screen, `options_cfg` session)
10 items, top to bottom: Player, Graphic, BGM, Vol, S.E.Vol, Mode, SlowMode, Reset, KeyConfig, Quit (logic: 15 §1.3).
Values in the live config (0x17CE870 + off): +0x1C starting lives (shown as 1..7 = lives + 1; 0..`max`, max limited
by the play count 0x164CED0 < 30 / < 60), +0x1E Graphic 32Bit/16Bit, +0x1F music mode (Off, Wav, Midi; changing it
restarts `bgm/th08_01.mid` in slot 8), +0x27 BGM volume and +0x28 SE volume (0–100; a press steps ±5, holding ±1
per frame after 30 frames: 15 §1.3), +0x22 Mode FullScreen/Window, +0x25 SlowMode Off/On, Reset (lives 2, +0x1D 3,
music 1, +0x20 1, slow 0), KeyConfig (screen 3), Quit. SE 0x1D plays every 50 frames while the cursor is on Vol or
S.E.Vol. Value display = one VM per value, lit = VM base sprite, dim = base + 1:
VMs 20–26 lives 0–6 (25/26 hidden below 30/60 plays), 27–28 colour, 29–31 music, 32–35 BGM volume digits
(sprite base + digit·2), 36–39 SE volume, 40–41 window, 42–43 slow (changelog/answers_impl9.md §16).

### 2.8 Replay (screen 7) — VERIFIED
Lists `./replay/th8_%.2d.rpy` (No.01…) and `th8_ud????.rpy` ("User …") files; choosing one loads it and sets
difficulty (+0x6B), shot (+0x6A), spell id (+0x7C), the chosen stage, flags |= replay, scene 2. Logic and
sub-states: 15 §3.8.

Menu fields: +0xC280 loaded replay (sub-states 2/3), +0xC288 file count, +0xC28C chosen/current list row,
+0xC290 chosen stage row, +0x7870 + 8·i label `No.%.2d` / `User `, +0x7A50 + 0x134·i copy of the first 0x134
bytes of file i (header: +0x6A shot, +0x6B difficulty, +0x6C date, +0x72 name, +0x7C spell id, +0x7E card name),
+0x70 + 0x200·i path.

Interrupts sent by the screen (title01 VMs 77–110, positions from the anm scripts):

| when | interrupt | effect on the VMs used for text anchors |
|---|---|---|
| enter (frame 0) and back to the list | 14 to all | VM 79 → (24, 128); VM 80 + k → (24, 160 + 17k), k = 0..14 |
| choose a file | 15 to all, then 17 to VM 80 + (row mod 15) | the other rows leave to y −32; the chosen row VM moves to (24, 160); VM 78 → (24, 192); VM 95 + j → (24, 256 + 17j) |
| stage chosen → speed | 19 to all, 17 to the row VM, then 21 to VMs 108/109/110 and 20 to VM 108 + cursor | 108/109/110 = speed plates (replay00.png sprites 111/112/113) at (384, 256/304/352); 20 = selected (moves to −4,−4, alpha → 255 over 10 frames), 21 = not selected (alpha → 96). In spell replays VM 110's alpha byte is set to 0 (hidden) |
| leave | 16 to all | |

Sub-state 3 cursor: `Menu_CursorUpDown(3)`, or 2 for spell replays; on every change the 21/20 interrupts above
are resent.

**Draw** (`MainMenu_ReplayDraw` 0x46EF8D, ref `menu_ref/mopup1_rep_list.png`, `…_rep_detail.png`,
`…_rep_speed.png`). Every string uses the ASCII queue (§2.9); "at VM k" = VM k's current raw pos.
1. Header `No.   Name       Date  Player   Rank` at VM 79.
2. Page = rows ⌊cur/15⌋·15 … +14 (cur = +0xC28C), only rows < count. Row j of the page at VM 80 + j:
   `"%s %8s  %6s %7s  %8s"` = label, name, date, character (0x4C86C0: `Rm & Yk`, `Ms & Al`, `Sk & Rr`,
   `Ym & Yy`, `Reimu  `, `Yukari `, `Marisa `, `Alice  `, `Sakuya `, `Remilia`, `Youmu  `, `Yuyuko `),
   difficulty (0x4C86F0: `Easy    `, `Normal  `, `Hard    `, `Lunatic `, `Extra   `). Spell replays
   (id ≥ 0): `"%s %8s  %6s %7s  Spell No.%3d"` with id + 1. Current row white **with highlight**, others
   0xFF808080 without.
3. Only in sub-states 2 and 3 (a replay is loaded):
   - `"       %2.3f%%"` (slowdown %, header +0x118 f32) at VM 78, white. The label 処理落ち率 is part of VM 78's sprite.
   - `Stage    LastScore` at VM 95.
   - Normal replays: 9 rows s = 0..8 at VM 96 + s: `"%s %9d0"` (stage name table 0x4C8250, 9 entries, last
     `StageEX`; score = stage record +0 ÷10, the score at the end of that stage, 15 §3.4) if the stage record exists,
     else `"%s ----------"`. Sub-state 2: chosen stage row white with highlight, others 0xFF808080. Sub-state 3:
     no highlight, chosen 0x60FFFFFF, others 0x60808080.
   - Spell replays: `"No.%.3d %9d0"` (id + 1, header +0xB0) at VM 97, colour/highlight as left over from the
     list (white, no highlight). Then scratch VM menu+0xE7B8 drawn at VM 97's pos + (0, 16): it holds the card
     name (header +0x7E) rendered at selection time with `Text_RenderToVm` (white, outline 0, font 15) into
     `resulttext.anm` sprite 11 (512×17 strip at y 162), anchor top-left, colour white.
4. At the end the queue colour is reset to white and highlight off.

### 2.9 ASCII string queue (`Ascii_Printf` 0x402A30 / `Ascii_QueueString` 0x402920, drawn by `Ascii_DrawStrings` 0x402B20) — VERIFIED

Menu lists, the result screens and the HUD print text through the ASCII manager (0x4CCE20). A call formats the
string (`vsprintf`, ≤ 512 bytes) and appends an entry to a per-frame queue (max 256 entries of 0x60 bytes,
excess ignored): text (≤ 64 bytes copied), position (x, y, z), and a **copy of the current state**:

| state field | global | default | meaning |
|---|---|---|---|
| +0x8268 | 0x4D5088 | 0xFFFFFFFF | colour (ARGB). Set by the callers directly or with `Ascii_SetColor` 0x4398E8 |
| +0x826C / +0x8270 | 0x4D508C / 0x4D5090 | 1.0 / 1.0 | glyph scale x / y (`Ascii_SetScale` 0x42F2F0) |
| +0x8274 | 0x4D5094 | 0 | 1 = clip to the playfield viewport while drawing |
| +0x8278 | 0x4D5098 | 0 | **highlight**: stored only when cfg+0x38 bit 0 or bit 8 is set. Bit 0 is always forced to 1 (15 §1.1), so in practice it always takes effect |
| +0x8280 | | 13 | advance per glyph in px (×scale x) |

Drawing (`Ascii_DrawStrings`, ASCII draw pass, after everything drawn by the menu/result object): one VM is
reused, anchor top-left. Pen = entry position. '\n' → y += 16·scale y, x = start; ' ' → x += advance; any other
byte c:
- normal: sprite `ascii.anm` index c − 1 (entry 0, `ascii.png`), colour = entry colour;
- highlight: sprite c + 0x8A (entry 1, `asciis.png`, index c − 22 inside that entry: a different, blue-white
  glyph set), colour forced to 0xFFFFFFFF (the entry colour is ignored);
- `Draw2DSnap`, x += advance.
Glyphs are 16×16 cells drawn 13 px apart (scaled). Scale applies to both the quad and the advance.
`Ascii_DrawGaugePercent` (§5.9) is **not** queued: it draws immediately in the caller's draw pass.

## 3. Pause menu (ASCII manager, `PauseMenu_Update` 0x4037B0, runs while paused) — VERIFIED

Opening: GM calc on ESC (11 §4). The ASCII manager calc (prio 1, 0x402200) runs this while `paused`, and
the continue menu while `game_over` (only one of each). The playfield is captured into `capture.anm`
(rect 32,16,384,448) when the capture option bit (0x17CE8FC bit 1) is set and drawn behind the menu.
**Texture capture** (VERIFIED code): `AnmManager_QueueCapture` 0x406A30(slot, src x, y, w, h, dst x, y, w, h) stores a
request at AnmManager +0x2A252C… only if none is pending (else it returns −1 and the pause/continue menu closes at
once, state 0). After the next Present, `AnmManager_ApplyPendingCaptures` 0x443B60 copies the back-buffer rect into
the texture of ANM slot `slot` entry 0 (`Anm_CaptureToTexture` 0x466F20, D3DX surface load with the default filter,
so a size mismatch is filtered). All four callers (pause menu, continue menu, `Hud_StageTransition`, `Msg_Run`
stage-end) pass slot 3 (`capture.anm`), source (32, 16, 384, 448) = the playfield, and as destination the capture
VM's current sprite rectangle truncated to int: (sprite +0x08 u0, +0x0C v0 in texture pixels, +0x34 width, +0x30
height) (structs.md AnmSprite). The capture therefore shows the frame presented just before the request.
VMs: `ascii.anm` scripts 12–21 on 10 menu VMs (title, items "resume / quit / retry", confirm "yes/no" …).
Colours: selected 0xFFFFFFFF, others 0xFF505050 (confirm screen: selected 0xFFFF8080); the selected item is
offset (−4, −4).
**Draw** (0x404750, ASCII manager draw, only while paused 0x164D0BA ≠ 0; VERIFIED): flush the batch, viewport =
playfield (32, 16, 384, 448); if the capture option bit is set and the pause state ≠ 0, draw a temporary copy of the
capture VM (+0x1A70) with VM flag bit 13 set (z-write off) via `Draw2DSnap`; then `Draw2DSnap` each visible menu VM
+8 + i·0x2A4, i = 0..9.

| state | meaning | input (from frame 4 of the state) |
|---|---|---|
| 0 | open: start VMs, set difficulty sprite (VM +0x1284: `0x11B + difficulty`; sprite **0x120** only in spell practice with card id ≥ 205 (0xCD); spell practice below 205 shows the card's own difficulty, VERIFIED 0x4037B0, RE-Q25), → 1 | |
| 1 | cursor on **Resume** | UP → 3 (2 in replay), DOWN → 2; confirm → 4 |
| 2 | cursor on **Quit to title** | UP → 1, DOWN → 3 (1 in replay); confirm → 6 |
| 3 | cursor on **Retry** (hidden in replay) | UP → 2, DOWN → 1; confirm → 8 |
| 5 / 6 | quit: "yes" / "no" | UP/DOWN toggle; confirm on 5 → 9, on 6 → back to 2 |
| 7 / 8 | retry: "yes" / "no" | confirm on 7 → 10, on 8 → back to 3 |
| 4 | closing | after 20 frames: paused = 0, sound resumes |
| 9 | quit | after 20 frames: scene 1, paused = 0, `Result_Register(2)` (save) |
| 10 | retry | after 20 frames: spell practice / practice / Extra → stop music (or restart spell BGM), scene 11; else scene 10 |

Shortcut keys at any time: ESC → close (4); Q → quit (9); R → retry (10, not in replay). All confirms play
sfx 10, cursor moves sfx 0 (`se_plst00`).

## 4. Continue / game-over menu (`ContinueMenu_Update` 0x404890) — VERIFIED
Flow and side effects: 11 §4.2. UI: `ascii.anm` scripts 22–24 on three VMs ("continue?", "yes", "no"),
`times.anm` script 1 with sprite = clock counter (the night clock, advanced by one at frame 15 of a continue);
selected item colour 0xFFFF8080, other 0xFF505050; default cursor "No" (state 2) in normal games and Extra; "Yes"
(state 1) only in spell practice when the card was not captured (spell flag bit 9 clear) (VERIFIED 0x404890, corrected Phase 5). Input from frame 30 (state 2) / frame 4 (state 1); UP/DOWN toggle with sfx 0; confirm sfx 10.

**Draw** (`ContinueMenu_Draw` 0x4052B0, from the ASCII manager draw, only while game_over 0x164D0BB ≠ 0; VERIFIED):
flush the sprite batch, set the D3D viewport to the playfield (x 32, y 16, w 384, h 448 from 0x164D2DC…0x164D2E8,
truncated); if the capture option bit (0x17CE8FC bit 1) is set and (state ≠ 0 or the state frame > 2), draw the
captured playfield VM (menu +0xFE0) with `Draw2DSnap`; then `Draw2DSnap` each visible menu VM at +8 + i·0x2A4:
i = 0..3 (continue?, yes, no, night clock) in a normal game below Extra, i = 0..2 (no clock) in spell practice or
Extra. Positions and fades come from the VMs' scripts (ascii.anm 22–24, times.anm 1).

## 5. HUD (`g_Hud` 0x160F428, calc prio 15 `Hud_Calc` 0x4338CA, draw prio 17 `Hud_Draw` 0x433927)

### 5.1 HUD object — VERIFIED
| off | meaning |
|---|---|
| +0x00 | frame counter |
| +0x04 | dirty counters, 2 bits each: 0–1 lives, 2–3 bombs, 4–5 power, 6–7 graze, 8–9 point items, 10–11 time (set to 2 = redraw in both back buffers) |
| +0x08 | impl block (VMs, dialogue 10, bonus panel) |
| +0x0C / 0x10 / 0x14 / 0x18 | `front.anm` (slot 10), `stgNtxt.anm` (13), `times.anm` (14), `loadingNN.anm` (12) |
| +0x1C | boss bar alpha 0–255 (+4/frame while shown, −4 while hiding) |
| +0x20 | boss "remaining bars" count (ECL op 148) |
| +0x24 / +0x28 | boss timer seconds / last value shown |
| +0x2C | boss bar requested (byte; ECL) |
| +0x30 / +0x34 | target life fraction (enemy calc: life/max) / displayed (moves +0.01 up, −0.02 down per frame) |
| +0x3C[8], +0x5C[8], +0x7C[8] | life-bar markers: end, start, colour (ECL op 158) |

Impl VMs: `front.anm` scripts 0–15 → VMs impl+i·0x2A4 (i = 0..15); `stgNtxt.anm` scripts 0–3 → impl+0x2A44
(stage title; in spell practice only script 3 with sprite = song id + 3); `ascii.anm` script 3 → impl+0x212C8
(stage-clear), script 0x1A → impl+0x5484, script 0x19 → impl+0x22E14 (difficulty name, sprite 0x11B + difficulty,
0x120 only for spell practice ids ≥ 205 — spell practice below 205 uses 0x11B + difficulty, the difficulty being
the card's (16 §6.2) (VERIFIED, RE-Q25)); `times.anm` → impl+0x2156C (night clock, ECL 179/181), impl+0x34D4 (clock,
sprite = counter).
Stage-title render state (VERIFIED): `stgNtxt.anm` scripts contain no `blend_mode` and no `zwrite_off`, so title
quads use the default VM state (alpha blend, zwrite 1, ztest always, colour = tex × col); colour changes only through
`color_time` (scripts 0/1 at t 970 → (80, 96, 144) over 120 frames) and alpha ops. Do not inherit the previous
sprite's blend state.
**impl+0x34D4 runs only while the stage title shows** (VERIFIED, changelog/answers_impl17.md §5.2): `Hud_Update` calls
`RunRange(impl+0x34D4, 1)` only when not spell practice (GameFlags bit 14 clear) **and** the alpha byte of the
stage-title VM 0 colour (impl+0x2A44 + 0x1F3 = `stgNtxt.anm` script 0) is ≠ 0; it is drawn every frame
(`Hud_DrawBoss`). times.anm script 0 fades in at its time 240, so the clock appears 240 run-frames after title
VM 0 starts its fade-in: demo 1 (stg4atxt script 0 fades in at 130) → clock first drawn at gameplay frame 369;
demo 0 (stg5txt script 0 fades in at 730) → 969. When the title VM's alpha returns to 0 the clock VM stops where
it is (stage 4A: last clock draw at frame 547 with alpha 4, exactly at the end of its own fade-out).

### 5.2 front.anm layout (positions are sprite centres) — VERIFIED from the anm data
| VM / script | sprite | pos | role (INFERRED from position) |
|---|---|---|---|
| 0 | 0 (128×256) | (480, 208) | side-panel background |
| 1 | 1 (128×31) | (512, 416) | logo plate (drawn rotating, `Draw2DRot`) |
| 2 | 2 | (416, 40) | "HiScore" |
| 3 | 3 | (448, 56) | "Score" |
| 4 | 4 | (432, 104) | "Player" label |
| 5 | 5 | (464, 104) | "Bomb" label |
| 6 | 6 | (424, 136) | "Power" |
| 7 | 7 | (432, 136) | — |
| 8 | 8 | (448, 168) | "Point" |
| 9 | 9 | (416, 184) | "Time" |
| 10 | 11 (16×16) | (placed by code) | life icon |
| 11 | 12 (16×16) | (placed by code) | bomb icon |
| 12 | 10 (128×16) | (80, 26) | "Enemy" label of the boss bar (interrupt 1 show, 2 hide) |
| 13 | 13 (32×32) | tiled | frame background tile |
| 14 | 14 (128×16) | tiled | top/bottom border strip |
| 15 | 15 (160×16) | (480, y) | erase patch behind a panel row |

`front.anm` entry 1 (`ename.png`, sprites 16–32): boss name plates (128×16).

**Boss name plate** (VERIFIED): front sprite 10 (128×16 at (0, 208) of front.png, empty in the file) is filled after
each stage load: `GameMgr_Calc` calls `Hud_SetStageNamePlate` 0x438046, which calls `Hud_SetBossNamePlate(n)` 0x437F5C;
that copies the rectangle of front.anm sprite n into sprite 10's rectangle of the front.png texture
(surface-to-surface copy 0x467040). front.anm script 12 shows it (pos (80, 26) → (32, 26), fade in 16 frames on
interrupt 1, out on interrupt −1). `Msg_Start` entry 0 also sets plates (10 §4). Practice = spell practice,
id = card id 0x164D0B8:

| stage idx | plate |
|---|---|
| 0 (default) | 0x10 |
| 1 / 2 / 4 / 6 | 0x11 / 0x12 / 0x14 / 0x17 |
| 3 | not practice or id 0xD6 → 0x13; ids 0xD8–0xDD → 0x1A–0x1F; other practice ids: **no call** (plate left as it was, QUIRK) |
| 5 | not practice or id 0xD4 → 0x15, else 0x16 |
| 7 | not practice or id 0x93–0x96 → 0x17, else 0x18 |
| 8 | not practice, id 0xBF–0xC1 or 0xD5 → 0x20, else 0x19 |

`ename.png` rows (16 px, 128 wide): 0x10 Wriggle, 0x11 Mystia, 0x12 Keine, 0x13 Reimu, 0x14 Marisa, 0x15 Tewi,
0x16 Reisen, 0x17 Eirin, 0x18 Kaguya, 0x19 Mokou, 0x1A Sakuya, 0x1B Youmu, 0x1C Alice, 0x1D Remilia, 0x1E Yuyuko,
0x1F Yukari; 0x20 = sprite 32, rect y 32 = Keine (the Extra plate).

### 5.3 Side panel drawing (`Hud_DrawPanel` 0x43625D) — VERIFIED
**Viewport during the HUD draw (VERIFIED code + golden draw log, RE-Q25).** `Hud_Draw` (prio 17) runs, in order,
`Hud_DrawClearPanel` (only queues ASCII strings), `Msg_Draw`, `Hud_DrawBoss`, `Hud_DrawPanel`, `Hud_DrawPopups`. Nothing
between `Stage_DrawLow` (06, which sets the playfield viewport 32, 16, 384, 448) and `Hud_DrawPanel` changes the
viewport, so every draw of draw prio 7–17 before `Hud_DrawPanel` — enemies, player, effects, bullets, the popups and the
gauge readout of `Ascii_DrawPopups` (prio 14, 13 §4 / §5.9: its glyphs at y > 464 are clipped), the Gui (15), the
dialogue, the stage title, clock and boss UI VMs — is clipped to the **playfield viewport**. `Hud_DrawPanel` flushes and
sets the full-screen viewport (0, 0, 640, 480) as its first action; the ASCII strings (prio 20) choose per entry (§2.9).
**Blank patches** (front.anm VM 15 = impl+0x279C, sprite 15, drawn with `Draw2DSnap`) — every frame unless cfg+0x38
bit 4 (Supervisor+0x150 bit 4, never set by the shipped cfg): (480, 40) and (480, 56) at z 0.49; (480, y) at z 0.48 for
each dirty row: y 88 (dirty bits 0–1), 104 (2–3), 136 (4–5), 152 (6–7), 168 (8–9), 184 (10–11); and **always (512, 464) at
z 0.48** (bottom-right corner under the fps text).
**Full repaint condition** (tiles, strips, VMs): cfg+0x38 bit 12, **or tile VM 13 (impl+0x2254) still has a running
script (ip ≠ NULL)** — front.anm script 13 ends with `stop` at time 200, so the first 200 HUD run frames after `Hud_Load`
always repaint (golden: 199 consecutive full-repaint frames from the stage start) — or the counter 0x17CE8CC ≠ 0. The
full repaint draws VMs 0 (snap), 1 (rot), 2–9 (snap) and the difficulty VM, then sets all six dirty fields to 2.
The original only repaints changed rows (dirty counters / full repaint counter 0x17CE8CC, writers below; the back buffer is kept between frames, see `specs/data/drawlog/FORMAT.md` §4); a reimplementation
may redraw everything every frame. Full repaint: tile 13 over x = 0 (y 0..464 step 32) and x 416..624 ×
y 16..464 (step 32); strip 14 at y 0 and 464 for x 0..624 step 128; then VMs 0–9 and the difficulty VM.
Each frame strip 14 is also drawn along y = 464 for x 32..368 (step 128).

ASCII text (`ascii.anm` glyph = sprite (char − 1), advance 13 px × scale, '\n' = +16 px):

| row | y | content |
|---|---|---|
| hiscore | 40 | x 488: `%.9d` of hiscore, x 605: `%1d` hiscore continues (≤ 9) |
| score | 56 | x 488: `%.9d` displayed score, x 605: `%1d` continues used (≤ 9) |
| lives | 88 | life icons (VM 10) at x 488 + 16·i, i < lives |
| bombs | 104 | bomb icons (VM 11) at x 488 + 16·i, i < bombs |
| power | 136 | gauge quad x 488 … 488 + power, y 136..152, colours 0xE0E0E0FF (left) / 0x80E0E0FF, then `%d` power, or "MAX" at 128 |
| graze | 152 | `%d` graze total |
| point | 168 | `%d` point items total, "/" at half width (+6 px), `%d` next extend |
| time | 184 | `%d` time, "/", `%d` time threshold; whole row colour 0xFFFFF0C0 when time ≥ threshold |

(Score digits have an implicit trailing 0: the internal score is ÷10.)

**Depth values (changelog/answers_impl15.md §Q8, VERIFIED).** pos.z set by `Hud_DrawPanel`: 0.49 border tiles, strips and the
hiscore/score row blanks; 0.48 the other row blanks; 0.46 life/bomb icons; ASCII text 0. zwrite = VM flag bit 13 clear
(front.anm scripts 0–12 have `zwrite_off`, tile scripts 13–15 do not). Invisible in practice (ztest always; depth is
cleared every frame), only relevant for draw-log parity.

**Full-repaint counter 0x17CE8CC** (VERIFIED): `Hud_DrawPanel` repaints the panel background in full while it is ≠ 0; `Frame_Present` decrements it once per presented frame while not paused (0x164D0BA == 0). Writers (value = number of full-repaint frames): 
- 2: `Gui_ShowPlayerSpellName` / `Gui_ShowBossSpellName` (spell-name plates), `Hud_ShowBonus`, `Hud_ShowMessage`, 
  `Hud_ShowSpellBonus`, `Frame_Present` after a failed Present + device reset; 
- 3: WinMain after a device reset; 
- 8: `ContinueMenu_Update`; `Hud_Calc` raises it to 8 while the Skip key (input 0x100) is held; 
- 0x10: `Hud_Load`; 0x3C: `GameManager_StageStart`; 0x1BA: `Msg_Run` (one dialogue op); 
- 0: `Supervisor_Init`.
A port that redraws the whole screen every frame can ignore it.

### 5.4 Boss UI (`Hud_DrawBoss` 0x43741D; only when no dialogue runs and the bar is shown or fading) — VERIFIED
- Life bar quad x 64 … 64 + 320·displayed_fraction, y 19..23, colour alpha|0xFFFFFF (top) / alpha|0x202060.
- Markers k (8 slots, enabled when end ≠ 0 and start < displayed): x 64 + 320·start … 64 + 320·min(end,
  displayed), colour alpha|colour, bottom alpha|((colour >> 2) & 0x3F3F3F).
- "Enemy" label VM 12 at (80, 26).
- Remaining-bars gauge: `n = +0x20` segments between x 35 and 61 (segment i: 35 + 26·i/n … 35 + 26·(i+1)/n −
  gap, gap 2 if n < 6 else 1), y 19..23, colour alpha|(0xFFFFFF − i·255/9).
- Timer `%.2d` (≤ 99) at (384, 16) coloured by value: ≥ 20 0xA0D0FF, ≥ 10 0xA080FF, ≥ 5 0xE080C0, < 5 0xFF4040
  (RGB, alpha = bar alpha); when the value changes: < 3 sfx 0x26 `se_timeout2`, < 10 sfx 0x1D `se_timeout`.
- Every frame when not paused, no continue menu, bullets not frozen (bit 10) and boss slot 0 (0xF54CC0) ≠ 0:
  `Popup_LargeStatic`(pos (2, 29) → screen (34, 45), p3 = number of chained children of the boss
  (`Enemy_ChainChildCount`), p4 = boss +0x3380, colour 0xFFF0F00F, manager scale (1, 1)) — a 2-frame large
  popup re-issued every frame; no count > 0 test, so it reads "+0" without children (VERIFIED, changelog/answers_impl10.md
  §1; 13 §4).

### 5.5 Popups (`Hud_DrawPopups` 0x438A89) — VERIFIED
All popup strings are queued with the ASCII clip flag +0x8274 = 1 (set at the start, cleared at the end), i.e. drawn in
the playfield viewport (§2.9).
| trigger | text | pos / duration |
|---|---|---|
| `Hud_ShowBonus(n)` 0x437DDD (spell-end bullet/enemy bonus, boss kill) | " BONUS %8d" colour 0xFFFFFF80 | starts (416, 48); x slides by −312 over 30 frames (to 104); 250 frames |
| `Hud_ShowMessage(v, type)` 0x437E5D | 1 "Full Power Mode!" (power items at max), 4 "Border Bonus %7d", 5 "Spell Bonus Failed", 6 "Last Spell Failed" (types 4–6 at 0.9 scale, advance 11) | (416, 168) sliding like above; 180 frames |
| `Hud_ShowSpellBonus(n)` 0x437EDC (capture) | "Spell Card Bonus!" red 0xFFFF0000 at x = (384 − 17·14)/2 + 32 = 105, y 80 (scale 1, advance 13); then "+%d" at y 96, x = (384 − len·28)/2 + 32, 2× scale, colour 0xFFFF8080 (VERIFIED 0x438A89, RE-Q25) | 280 frames |

### 5.6 Stage-clear panel (`Hud_DrawClearPanel` 0x43826B, while msg op 9 state is set) — VERIFIED (rewritten RE-Q25)
`X` = "a replay of a practice game" = `0x43C322`: GM flags bit 3 (replay) **and** info byte +0x7B of the loaded replay
(15 §3.5, the practice flag recorded at record start). Replay playback never sets GM flag bit 0 (15 §3.8), so practice
replays are recognised only through +0x7B. `P` = GM flag bit 0 (practice).

**Bonus computation** (`Hud_Calc`, once, on the first frame after op 9; all integer arithmetic):
1. `t = clear_bonus + graze·50 + point_items·5000 + time·100` (values stored by op 9, 10 §9).
2. Stage idx ≥ 6 and not `P`: `t += lives·2 500 000 + bombs·500 000` (replays included, even practice replays).
3. Stage idx 7 (6B): `t += (12 − clock)·2 000 000` — no practice test.
4. Difficulty: 0 → `t/2`; 2 → `t·12/10`; 3 → `t·15/10`; 4 → `t·2`; 1 and 5 unchanged.
5. Lives option (gameplay config +0x1C): 3 → `t·5/10`; 4 → `t·2/10`; 5 → `t/10`; 6 → `t/20` — **always** (no
   difficulty/practice test).
6. Store t (impl+0x22D88); `AddScore(t)` 10 times.

**Draw** (ASCII strings, clip flag 0 = full screen; pen starts at (120, 96); "y +n" = advance before the next line):
| step | condition | text | colour |
|---|---|---|---|
| title | stage < 6: "Stage Clear"; else "All Clear!" — for stage idx > 6 (6B, Extra) the title is at y 80 | | 0xFFFFFF40 |
| | | y +32: "Clear = %8d0" | 0xFFFFFFFF |
| | | y +16: "Point = %8d0" (points·5000) | 0xFFE0E0FF |
| | | y +16: "Graze = %8d0" (graze·50) | 0xFFD0D0FF |
| | | y +16: "Time  = %8d0" (time·100) | 0xFFD0D0FF |
| | | y +32: "over-80%% = %3d.%.2d%%" with (h·100)/f and ((h·10000)/f) % 100 (integer, truncated; h = human-max frames 0x164D32C, f = 0x164D31C) | 0xFFD0D0FF |
| | | y +16: "over 80%% = …" the same with youkai-max frames 0x164D328 | 0xFFD0D0FF |
| | stage ≥ 6, not `P`, not `X` | y +16: "Player =%8d0" (lives·2 500 000); y +16: "Bomb   = %7d0" (bombs·500 000) | 0xFFFFFF80 |
| | additionally stage 7 | y +16: "Last Time = %2d:%.2d" with (m/60) % 12, m % 60 of the new clock minutes m (impl+0x22E08 = clock·30 + 660); y +16 "Night Bonus"; y +16 "        %8d0" ((12 − clock)·2 000 000) | 0xFFFFFF80 |
| rank | | y +32: "Easy Rank    *0.5" / "Normal Rank  *1.0" / "Hard Rank    *1.2" / "Lunatic Rank *1.5" / "Extra Rank   *2.0" / "Phantasm Rank*2.0" (difficulty 5) | 0xFFFF8080 |
| penalty | difficulty < 4 and not `P` | y +16, then for option 3/4/5/6 "Player Penalty*0.5" / "*0.2" / "*0.1" / "*0.05" followed by y +16 | 0xFFFF8080 |
| total | | y +16: "Total = %8d0" (stored t) | 0xFFFFFFFF |
| clock | stage < 6 | y +40, x 120: "%s%2d:%.2d" of the old minutes (impl+0x22E04) in 0xFFDFDFDF; x +99: ">>" in 0xFFAFAFAF; x +34: the animated minutes (impl+0x22E0C) in 0xFFFF8F8F. Prefix "PM" while minutes/60 < 12, else "AM"; hours shown (minutes/60) % 12 | |

So the lines shown and the bonus actually added differ: practice replays add the lives/bombs bonus without showing
it, practice games and practice replays add the 6B night bonus without showing it, and the lives-option penalty is
applied in every game but shown only below Extra outside practice.
Clock animation (impl+0x22E10 frame counter): after 60 frames the displayed minutes rise by 1 per frame (+3 more while
SHOT or SKIP held, clamped to the target); when reached (stages 1–5, GM flags bits 5–6 clear) GM flags := bits 5–6 = 2
(the stage-clear trigger, 11 §4.1).
Counters for the percentage lines: 0x164D31C counts this stage's non-dialogue player frames, 0x164D32C / 0x164D328 the
frames at human / youkai max gauge; reset at stage start, incremented in `Player_Calc` together with **+100 points per
frame** at either max.

### 5.7 Other HUD elements
- Stage-start transition: 168 VMs (14 × 12 grid of 32-px `capture.anm` tiles, script 3/4 alternating,
  staggered by `row + 2·col`) drawn as 3D quads while active (impl+0x21810 counts live ones).
- Night clock VM impl+0x2156C (`times.anm`, VERIFIED RE-Q25): ECL op 179 (0x439007) starts script 2 on it with
  sprite = clock counter; op 181 sets sprite = clock and interrupt 1 (2 when the clock reaches 12) (07 §7); op 180 and
  `Hud_Load` (0x4390D6) set its alpha byte to 0. It is run every `Hud_Update` (after the stage-clear VM) and drawn
  every frame by `Hud_DrawBoss` (`Draw2DRot`, playfield viewport), right after the stage-title VMs and the clock
  impl+0x34D4.
- Player-overlap fade (VERIFIED, `Hud_Calc`): impl byte +0x2175F is the **alpha byte of the night-clock VM**
  (impl+0x2156C, `times.anm` script 2, ECL ops 179/181; op 180 = 0x4390D6 sets it to 0 = hidden). While the
  alpha is ≠ 0: if the player is at x ≥ 64 and y < 128 it drops by 4 per frame down to 64 (0x40); otherwise
  it rises by 4 per frame up to 255 (values ≥ 0xFC snap to 0xFF; 0xFF stays).
- Score/hiscore and the boss UI use the same ASCII manager queue (256 strings per frame, drawn at prio 14/20).

### 5.8 Spell-name plates, cut-ins, spell-bonus line (`g_Gui` 0x4EA670) — VERIFIED (changelog/answers_impl11.md §13)
The spell manager object is `g_Gui`; `Gui_Calc` (calc prio 12) = `Spell_Calc` 0x416B90, `Gui_Draw` (draw prio
15) = `Spell_Draw` 0x4178C0. It owns 14 VMs at +0x120 + k·0x2A4:

| k | offset | file / script (set by) | role |
|---|---|---|---|
| 0 | +0x120 | player face file slot 16 (human, EDX 0) / 17 (youkai, EDX 1), script 0, sprite 0 (`Gui_ShowPlayerSpellName`) | player cut-in |
| 1 | +0x3C4 | spell face file slot 18, script 0, sprite = face index (only if ≥ 0) (`Gui_ShowBossSpellName`) | boss cut-in |
| 2 | +0x668 | `face_cdbg.anm` (slot 15) script 0 (player) / 1 (boss), sprite = isLastSpell (player) / 0 (boss) | vertical streak strip |
| 4 | +0xBB0 | `face_cdbg.anm` script 2 (player) / 3 (boss), same sprite | horizontal streak strip |
| 6 | +0x10F8 | `text.anm` script 4 (load and every player show) | player card name |
| 7 | +0x139C | `text.anm` script 5, or 6 for card ids 0xCD–0xDD | boss card name |
| 8, 9 | +0x1640, +0x18E4 | `text.anm` scripts 7, 8 (only for ids 0xCD–0xDD) | boss name glow copies |
| 10 | +0x1B88 | `ascii.anm` script 1 (load) | player name plate |
| 11 | +0x1E2C | `ascii.anm` script 0 (load) | boss name plate |
| 12 | +0x20D0 | `ascii.anm` script 4 (load; sprite 136, stop) | digit stamp for the bonus line |
| 13 | +0x2374 | `ascii.anm` script 2 (load) | "Bonus … history" label |
| 3, 5 | +0x90C, +0xE54 | never given a script (inert; drawn in the boss block but invisible) | — |

`Gui_LoadFaces` also clears the ip (+0x220) and the visible bit (flag bit 0) of VMs 0–7 and sets VM 6/7 font size bytes
(+0x298/+0x299) to 15 (the copies 8/9 keep 0; the boss renderer then uses 15 for the width, INFERRED effect on
the height).

**Player show** `Gui_ShowPlayerSpellName(face, name, isLastSpell)` (from `Bomb_Start`, EDX = face: 0 for
Reimu/Marisa/Sakuya/Youmu bombs, 1 for Yukari/Alice/Remilia/Yuyuko, −1 for the dissolve spell → VM 0 not
touched; isLastSpell = the 4th `Bomb_Start` argument — note Youmu LS passes 0, QUIRK): VMs 0/2/4 as above;
VM 6 script 4 then the name is rendered into its sprite (`Text_RenderToVm`, COLORREF 0x00F0F0FF = RGB
(255, 240, 240), outline 0, left-aligned in the 320×17 strip (0, 90)); +0x2618 := len·15/2 + 16 (never read);
VM 10 interrupt 1; SE 0xE; 0x17CE8CC := 2. **Player hide** 0x416130 (`Player_UpdateBomb`, bomb end): VM 6
pending interrupt 1, VM 10 interrupt 2.
**Boss show** `Gui_ShowBossSpellName(face, name, 0)` (`Spell_Start`, name XOR 0xAA decoded; native 22 name-only
via 0x415D10): VM 1 if face ≥ 0; VM 2 script 1, VM 4 script 3 (sprite 0); VM 7 (and 8/9) scripts; name
rendered **right-aligned** (0x4664A0: x = strip.x + strip.w − len·size/2, size 15) in COLORREF 0x00FFF0F0 =
RGB (240, 240, 255), outline 0; +0x261C := len·15/2 + 16 (never read); VM 11 interrupt 1; VM 13 interrupt 1
unless spell flag bit 10 (name only); SE 0xE. **Boss hide** `Spell_HideName` (native 23): VM 7 pending
interrupt 1, VM 11 interrupt 2, VM 13 interrupt 2 (unless bit 10). Nothing else interrupts VM 7, so the boss
name stays parked at its top position until native 23 or the next show.

Calc: all VMs (0, 2, 4, 6, 10, 1, 3, 5, 7, 8, 9, 11, 13 in this order) run only when bullets are not frozen
(GameFlags bit 10) and the freeze byte 0x160F534 is 0.
Draw (`Spell_Draw`):
1. If VM 0 is visible: VM 0 Draw2DSnap, VM 2 Draw2DSnap, VM 4 Draw2DRot. The boss strips (VM 2/4 scripts 1/3)
   are therefore **only drawn while a player cut-in is visible** (QUIRK; matches the runtime ref
   `spell54_cutin_f134.png`: no strips behind the boss cut-in).
2. If VM 1 is visible: VM 1 Draw2DSnap at pos + pos_offset (+0x288, restored afterwards), VMs 3/5 (inert).
3. If VM 6 is visible: VM 10 pos := VM 6 pos − (32, 0); VM 10 Draw2DSnap; VM 6 Draw2DRot.
4. If VM 7 is visible: global tint on with +0x2620 (= 0x80808080 from `Spell_Start`, neutral); VM 11 pos :=
   VM 7 pos; VM 11 Draw2DSnap; VM 7, 8, 9 Draw2DRot; VM 13 Draw2DSnap; then unless spell flag bit 10 the
   bonus line: start = VM 13 pos + (−40, +1); bonus = +0xFC if spell flag bit 2 (still capturable) else 0,
   8 digits from 10 000 000 with leading zeros suppressed (units always), each digit = VM 12 with sprite
   0x88 + d (`ascii.anm` 8×12 digits), advance 7; then x += 32: captures of this card (+8) for the current
   shot type (CATK story +0x158 / practice +0x1F4, capped 999), 3 digits, leading zeros suppressed, advance 7;
   then x += 13: attempts (CATK +0x124 / +0x1C0, capped 999) the same way, except that its tens digit is also
   forced when the captures had a hundreds digit. Finally the global tint is turned off.

Scripts (all positions are screen coordinates):
- Player cut-in (all 8 `face_XX00.anm` script 0): top-left anchor, pos (48, 144) → (48, −112) linear over 180
  frames, alpha 0 → 128 over 30, at 90 → 0 over 60, hidden at 150.
- Boss cut-in (`face_*sp.anm` script 0): top-left, pos (X, −112) → (X, 144) over 180, alpha 0 → 192 over 30,
  at 90 → 0 over 60, hidden at 150; X = 160 (most), 140 (st08sp), 80 (st05msp, st06sp, st07sp), −32 (yksp).
- `face_cdbg.anm` (sprites 0 = (1, 0, 94×512), 1 = (97, 0, 94×512)): script 0 additive, top-left, (112, 16) →
  (224, 16) over 150, alpha 0 → 255 over 50, v scroll −1/120 per frame for 100 frames, then alpha → 0 over 50
  while scrolling 50 more frames, hidden; script 1 (normal blend) the same from (336, 16) → (240, 16), scroll
  +1/120 (100 + 50 frames); script 2 additive, rotated −π/2 (centred), (224, 264) → (224, 360) over 150, from
  t = 20 alpha 0 → 255 over 50, scroll +1/120 for 80 frames, then fade 50; script 3 (normal) the same from
  (224, 408) → (224, 312), scroll −1/120.
- `text.anm` script 4 (sprite 4 = strip (0, 90, 320, 17), centred): pos (208, 344), alpha 0 → 255 over 30,
  scale 3 → 1 over 30 (linear); t = 100: pos → (208, 440) over 30 (mode 4); waits; interrupt 1: pos →
  (−128, 440) over 30 (mode 4), hidden.
- `text.anm` script 5 (sprite 5 = strip (0, 108, 320, 17)): pos (256, 312), alpha 0 → 255 over 30, scale 3,
  scale growth −1/15 per frame for 30 frames then scale 1; t = 100: pos → (256, 40) over 30 (mode 4); waits;
  interrupt 1: → (576, 40) over 30, hidden. Script 6: the same with scale 5 → 1 over 50 (mode 4). Scripts 7/8
  (additive copies): from t = 10 / 20 alpha 0 → 255 over 30 and scale 5 → 1 over 50 (mode 4); at t = 60 / 70
  alpha → 20 instantly; hidden at 80 / 90 (they never move up).
- `ascii.anm` script 1 (player plate, sprite 134 = (0, 160, 256×32)) and 0 (boss plate, sprite 133 =
  (0, 128, 256×32)): hidden until interrupt 1; then alpha 0 → 255 over 30 (their own pos moves are
  overridden by the draw's copy of the text position); interrupt 2 → hidden after 30 frames. Script 2 (label,
  sprite 135 = (0, 192, 144×16)): interrupt 1 → alpha 0 → 255 over 30, pos (308, 344) → (308, 324) over 30
  (from t = 30), t = 100 → (308, 52) over 30 (mode 4); interrupt 2 → alpha → 0 over 30, hidden.

### 5.9 Human/youkai gauge readout (bottom-left of the playfield) — VERIFIED (changelog/answers_bugs2.md §C3b)
Ref: `specs/data/ref/c3_demo0_f1400_gauge_human_max.png` ("−100.00%", blue) and
`c3_demo0_f2200_gauge_youkai_max_timeorbs.png` ("82.76%", red). All coordinates are **screen** coordinates (no
playfield origin added).

VMs live in the ASCII manager (0x4CCE20). They use `ascii.anm` entry 0 (`ascii.png` 256×256) and are run every
frame by `Ascii_Calc` (also while paused):

| VM | offset | script | sprite | role |
|---|---|---|---|---|
| G0 | +0x7EC | 5 | 155 (128×16, line with swirl ends) | bar, top-left anchor |
| G1 | +0xA90 | 6 | 153 (16×16 「人」) | human end, top-left, pos (88, 449) |
| G2 | +0xD34 | 7 | 154 (16×16 「妖」) | youkai end, top-left, pos (88, 449) |
| G3 | +0xFD8 | 8 | 152 (8×12 ▼ heart) | marker, centred, pos (144, 453) |
| G4 | +0x127C | 4 | 136 | glyph stamp (sprite changed per glyph), centred |

Glyph sprites: 136–145 = '0'–'9', 146 '%', 147 '.', 148 '-' (8×12 each).

**Setup** (`Ascii_SetupGaugeVms` 0x402600). Called from `AsciiManager_Load`, from every `GameManager_StageStart`
and from `MainMenu_OnRegister`. In StageStart it runs near the end (after the Gui registration, right after
`Ascii_InitVms`, which does not touch these VMs). That is **after** `Player_Load`, which runs synchronously inside
`Player_Register` at the top of StageStart and writes the limits. So the setup always uses the limits of the game
being started (VERIFIED, changelog/answers_impl13.md §1). Steps:
1. Restart scripts G0 5, G2 7, G1 6, G3 8, G4 4 with `AnmFile_SetScript`. This does **not** zero pos (only
   `AnmManager_SetScript` does, 05 §8), so G0/G3 keep their last position. The restart runs the script once
   immediately (`AnmFile_StartScript`), so G1/G2 are at (88, 449) again before step 2 (no accumulation).
   The same call also restarts +0x1520 with script 9 (sprite 156, a pulsing 48×16 plate; it is run every
   `Ascii_Calc` but **never drawn** — no draw call references ASCII +0x1520, RE-Q25) and the four boss markers
   +0x17C4/+0x1A68/+0x1D0C/+0x1FB0 with script 10 (§5.10).
2. G1.x −= 56·min/−10000 and G2.x += 56·max/10000, with min/max = gauge limits 0x164D300/0x164D302 (09 §9.3).
   That gives 人 x = 32 and 妖 x = 144 by default; shot 3: 60/144; shot 10: 60/116; solo human: 32/99.2; solo
   youkai: 76.8/144. The limits are written only by `Player_Load`. They are 0/0 at boot (both ends x = 88) and keep
   the last game's values in the menu. Neither is visible, because the readout is hidden there (changelog/answers_impl13.md §3).
3. Re-send the stored interrupt (+0x827C) to G0–G3. At boot it is 0, which has no label: the VMs stay hidden.

**Timing at stage start (changelog/answers_impl15.md §Q7, VERIFIED).** The setup runs on the loader thread, but `Ascii_Calc` has calc
prio 1 and keeps running during loading (GM prio 2 returns 3), so label 1 is already k frames old at game frame 1;
k depends on the loader race: 28 / 27 / 29 / 33 in the demo 0 / 1 / 2 / 3 reference logs (slide ends at gm 12/13/11/7,
人/妖/▼ appear at k = 61). Port: run the setup at loading frame 81 − k (replay loading lasts 81 frames); k = 28 if one
constant is wanted.

**Interrupts** (`Ascii_GaugeInterrupt` 0x4070B0 sends to G0–G3 and stores the id in +0x827C):
- 1 = show: `Player_Load` on a fresh start (0x17CE8BC ≠ 0, 11 §3). It is sent before the limits are written,
  and step 3 of the setup sends it again later.
- 99 = hide (catch-all label): player on-remove 0x44DC60 when the full-exit flag 0x17CE8C0 ≠ 0. That is any
  leave except next stage (3) and same-stage reload (11; 12 is never requested). The supervisor turns 11/12 into 3
  before the GM is removed (11 §7). After 3/11 the last id (1/2/3) is just re-sent by step 3.
  **Retry (10)** is a full exit: 99 hides the readout, then the new GM is a fresh start and `Player_Load` sends
  1, so the readout fades in again exactly like at a new game (VERIFIED, changelog/answers_impl13.md §2).
- 2/3 = player-overlap fade, `Player_UpdateGaugeFade` 0x44D420, called every frame in `Player_Calc` after the shot
  timer. With the player pos (playfield coords +0x2B4/+0x2B8) and cur = +0x827C:
  - y < 400: if cur = 2 → 3.
  - y ≥ 400: if cur ≠ 2 and x < 160 → 2; else if cur = 2 and x > 160 → 3.
  - This function also sets player +0xE2AA4 and +0xE2AB0 to (−999, −999, 0) and +0xE2AC0 = 0 first.

Script timing (anm data):
- G0: label 1 = alpha 0→255 in 15 frames and pos (32, 465) → (32, 449) linear over 40.
- G1–G3: label 1 = alpha 0, then from frame 60 alpha 0→255 over 40 (G3 pos (144, 453)).
- All four: label 2 = alpha → 64 over 15; label 3 = alpha → 255 over 15.

**Draw** (end of `Ascii_DrawPopups` 0x405420, draw prio 14, after the popups and after the shake offset is zeroed).
Nothing is drawn unless G0 is visible (flags bit 0). g = gauge (stats +0x22).
1. G3.x = G0.x + 64 + g·56/10000 (float), drawn with `Draw2DRotNoSnap` (no snapping).
2. Zone colour RGB:
   - human max (`Gauge_IsHumanMax`): (0x70, 0x70, 0xFF);
   - else human: (0xB0, 0xB0, 0xFF);
   - else youkai max: (0xFF, 0x70, 0x70);
   - else youkai: (0xFF, 0xB0, 0xB0);
   - else white.
   Alpha = G0's current alpha. This dword becomes **G0's colour** (the bar is tinted) and G4's colour. Then G0,
   G1, G2 are drawn (`Draw2DSnap`). G1–G3 keep their own white colour.
3. Percent text (`Ascii_DrawGaugePercent` 0x405E10, also used by the spell-practice list 0x46F5CB):
   - Anchor P = (G0.x + 64 + g·40/10000, G3.y − 7). Let a = |g|.
   - Glyph count n = 4 + (g < 0) + (a < 1000 ? 1 : a < 10000 ? 2 : 3). Start x = P.x − (3.5·n − 3.5 − 4).
   - Each glyph is G4 with that sprite, `Draw2DSnap` at the current x, then x += the advance:
     - g < 0: '-' +7.
     - a < 1000: digit a/100 +7, '.' +5, then **scale 0.8 and y + 2**: digit (a%100)/10 +5, digit a%10 +7.
     - a < 10000: a/1000 +7, (a%1000)/100 +7, '.' +5, scale 0.8 / y + 2: (a%100)/10 +5, a%10 +7.
     - a ≥ 10000: fixed '1' '0' '0' (+7 each), '.' +5, scale 0.8 / y + 2: '0' +5, '0' +7.
   - Finally scale 1, y − 2, '%' (no advance).
   - So the text is g/100 with two decimals: "−75.48%", "0.05%", "100.00%". The two decimals are 0.8× and 2 px
     lower; the '%' is full size.
4. Max point value (stats +0x24, the point-item base value, 08 §8.5):
   - Start x = G0.x + 62 − 14, y = G0.y + 3 + 8 (centred glyphs), scale 1, same colour as step 2.
   - 8 digit positions, divisor 10 000 000 down to 1. A digit is drawn only once the running sum of the digits
     so far is ≠ 0 (leading zeros suppressed; value 0 draws nothing). Each drawn digit advances +7.

### 5.10 Boss position markers ("ENEMY", bottom edge of the playfield) — VERIFIED (RE-Q25)
Four VMs in the ASCII manager, M_k = +0x17C4 + k·0x2A4 (k = 0..3 = boss slot, 07 §4 e+0x3313), `ascii.anm` script 10:
sprite 157 (48×16 "ENEMY" plate at (0, 240)), z-write off, alpha 0, hidden until interrupt 1; label 1: alpha 0 → 255
over 30 frames, then wait; label 2: alpha → 0 over 30, hidden; catch-all (−1): hidden at once. Restarted by
`Ascii_SetupGaugeVms` (§5.9 step 1, every stage start) and run every `Ascii_Calc` (also while paused).
Interrupts (`0x422BB0`): op 127 slot ≥ 0 → 1 to M_slot; op 127 −1 and `Enemy_Free` of a boss (slot < 4) → 2, and
M_slot pos := (−999, −999, 0); `Player_Load` → 2 to M_0, M_1, M_2; player on-remove with the full-exit flag → 99 (→ hidden)
to M_0..M_2 (M_3 never gets these two).
Per frame in `EnemyMgr_Calc`, for every enemy with the boss flag (flags A bit 1), after the damage/hit-flash step
(07 §10.1 step 7): M_slot pos := (x + 32, 472, 0) (x = enemy final pos x; **−999** instead when flags A bit 4 is set),
and the marker mode +0x2254[slot] := flags B bits 4–5 (low-life warning level, 07 §10.2) + 1 if that level is ≠ 0,
else 1 if the enemy's main VM colour2 flag (bit 17, the hit flash) is set, else 0.
**Draw** (end of `Ascii_DrawStrings`, prio 20, after all queued strings; full-screen viewport): for k = 0..3, only if
56 ≤ M_k.x ≤ 392: d = |M_k.x − 32 − player x| (player pos 0x17D61AC, playfield coords); M_k sprite pointer := sprite 157
(0x9D) — set directly, without `SetSprite`; colour by mode:
- 0: RGB white, alpha = d < 64 ? trunc(d + 96) : 160 (the marker fades when the player is under the boss);
- 1: 0x80FF4040 (hit flash);
- 2 / 3 / 4: on frames where the ASCII frame counter +0x8284 (incremented at the end of every `Ascii_Calc`) % 8 / % 4 /
  % 2 == 0: sprite 158 (0x9E, the 48×16 plate at (48, 240)) and colour 0xFFFFFFFF; on the other frames as mode 0.
Then `Draw2DSnap(M_k)` (still subject to the VM's visible bit and alpha ≠ 0). The colour bytes are overwritten by the
draw every frame, so after the 30-frame fade-in the script alpha no longer matters. Golden draw log: `ascii.anm#0`
quad (199.5, 463.5)–(247.5, 479.5), uv (0, 0.9375)–(0.1875, 1), colour 0x60FFFFFF / 0x80FF4040 during the stage-1 boss.

## 6. Answers to deferred implementer questions
- Main menu scripts, highlighting and interrupts: §2.1–2.2 (all 142 `title01.anm` scripts are started at once;
  highlight = base sprite vs base+1; interrupts listed).
- Where/when `th08logo.jpg` is shown: §1 (surface slot 8 during boot loading until the title is ready).
- Op 59 with n = 0 does not draw from the RNG; RNG algorithm/seed: 11 §1.
