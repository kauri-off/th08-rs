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
- What ends each hold (VERIFIED, RE-Q40): (1) **boot**: `Supervisor_Calc` returns 1 at its top while the
  `Supervisor_Init` loader thread runs (11 §7); the first calc after the thread has finished does the 0 → 1 switch and
  registers the main menu, whose on-register starts `MainMenu_LoaderThread` and sets MainMenu+0x14434 := 1;
  (2) **menu load**: while +0x14434 = 1 `MainMenu_Calc` returns 1 and runs nothing (no screen, no VMs; 2 = load
  failed → return 4); the thread, after loading, starts the title BGM (when not in demo mode and the previous scene
  0x17CE8B8 ≠ 5 result viewer), registers the fade-in ScreenEffect, sets +0x14434 := 0 and calls `Loading_End`; the next menu calc runs
  the top screen. Both lengths are host thread times (file decode + texture creation), with no frame count in the
  code; a reference run under wine measured 13 boot calc frames and the menu loader ending at scene frame 34 (33 when
  the menu is entered again). A port that loads synchronously may use any fixed counts; these change only the number
  of logo / loading frames, not the game logic.
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
screen counter +0x1442C at the end of each call; a press that starts a delayed switch zeroes +0x10 in its own call
(not every press: e.g. the replay screen's file confirm and its inner cancels leave +0x10 alone, see below). A test "+0x10 > 19" (> 29,
> 59) is therefore true in the call P + 20 (P + 30, P + 60) after the press on frame P, which calls `Menu_SetScreen`
(return 1), and the next screen first runs on P + 21 (P + 31). Uses: difficulty / spell-practice-char cancel 20,
character cancel 30, replay leave 30 (top cursor 4), Quit 60. **Exception, option item 7** (VERIFIED 0x4674E0 +
`options_cfg` trace rows 327 → 357, RE-Q37): the confirm sets sub-state 3, zeroes +0x10 and calls `MainMenu_Option`
in place; with sub-state ≠ 0/1 that call only increments +0xC294/+0x10/+0x1442C, and the top handler increments them
again at its end, so +0x10 = 2 after the press call: the switch to screen 2 happens in call P + 29 and the option
screen first runs on P + 30. Input start: screens 4/8/12, 5/9/13/15,
11, 16, 17 switch to their input sub-state in the call where +0x1442C == 8 (input from that call on); the replay
screen leaves sub-state 0 when +0x1442C > 7 and accepts confirm/cancel when +0x10 > 9; the option screen moves the
cursor from its first call and accepts Left/Right/confirm/cancel from +0x1442C ≥ 4; the top menu: see §2.2 "Input
timing" (cursor from call 9, confirm/cancel from call 19 of each entry). Replay screen in call numbers (call 0 = the
entry call): sub-state 1 starts in call 8 (+0x10 := 0 there), its first call is call 9 (cursor Up/Down and Left/Right
paging from call 9), confirm/cancel from call 18 (+0x10 > 9) (VERIFIED 0x46E136, RE-Q37). The +0x10 > 9 gate exists only in sub-state 1 and is armed
once: after sub-state 1 starts, +0x10 is zeroed again only by the list's cancel (→ sub-state 4, leave). The file
confirm (→ 2) leaves +0x10 counting, and the cancels 2 → 1 and 3 → 2 zero +0x1442C, not +0x10; so back on the list
the gate is already open and confirm/cancel work from the first call. The replay screen's entry block sits **inside
the sub-state-0 case** (guard = sub-state 0 and +0x1442C == 0), so zeroing +0x1442C in sub-states 2/3 never re-runs it;
no code reads +0x1442C in sub-states 1–4 (only those two cancels zero it and the common tail increments it), so its
value there is unobservable (VERIFIED disasm 0x46E156/0x46E19E, dispscan +0x1442C, RE-Q47). Sub-states 2 and 3 have no gate of their own:
cursor, confirm and cancel from their first call (the call after the press). Sub-state 4 leaves when +0x10 > 29
(VERIFIED 0x46E136, RE-Q42).

### 2.1 title01.anm VM map (script index = VM index) — VERIFIED from the anm dump

| VMs | content |
|---|---|
| 0 | background (`title02.png`) at x 384 |
| 1–9 | top-menu items 0–8 at (64, 180 + 28·i); each item has two sprites: **selected = the script's own sprite, unselected = sprite + 1** |
| 10–43 | option menu labels/values (VM 43 = SlowMode "On", label 3) |
| 44–76 | key config (label 4; layout 15 §1.3 "Key config VMs") |
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
  to (640, 320), hidden. Every way into screens 11/16 passes such an interrupt first (top 2/5, screen 9/15 entry 8,
  screen 17 entry 26), so VM 141 starts its label-18 slide (`pos_time` 20 frames, mode 4) from (640, 320); the screen
  handlers never write its pos. Exception (VERIFIED from 05 §4/§6, RE-Q37): the catch-all's `pos` does not stop a
  running pos interpolation, which overwrites pos again in the same POST. A screen 16 → 17 → 16 (or 11 → 9 → 11)
  round trip within the 20 frames of the label-18 slide therefore keeps sliding towards (256, 320) while hidden, and
  the next interrupt 18 starts its slide from that position, not from (640, 320).
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
(`Text_RenderCentered` 0x466650: x = trunc(256 − len·15/4) with len = byte length, computed in x87 floating point and
truncated toward 0, e.g. len 5 → 237.25 → 237, not integer `len·15/4` (RE-Q45); W = 512, 10 §6; text colour COLORREF
0x00FFF0E0 = RGB(224, 240, 255), 2-px black outline, font 15; RE-Q44).
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
| 7 Option | 各種設定できます | cursor 0, sub-state 3, +0x10 := 0, in-place `MainMenu_Option` call, cursor 0 again; **no range interrupt and no highlight** (the items slide out only on the option entry's range interrupt 3, call P + 30). The press call then falls through to the cancel test (a Bomb/ESC edge in the same call moves the cursor to Quit with SE 11) and the counter increments. Sub-state 3 calls run nothing but the test +0x10 > 29 and the counter increments (no cursor, highlight, change test, demo counter, description render or confirm/cancel). In call P + 29 the test switches inline (not via `Menu_SetScreen`): +0x68 := 0, screen 2, +0x10/+0x1442C/+0x0C/+0xC294 := 0, cursor := 0, config snapshot (+0x14454 := the 0x3C bytes at 0x17CE870), return 1; screen 2 first runs P + 30 (§2 intro) (VERIFIED 0x4674E0 + `options_cfg` draw log: items static and item 7 lit in presents 328–356, sliding from 357, RE-Q42) |
| 8 Quit | いろいろと終了します | range interrupt 1, sub-state 2; **no music change in WAV mode** (MIDI mode only: `Midi_Stop`, `Midi_Open(…, 30)`, `Midi_Play`; there is no fade, VERIFIED 0x4674E0, RE-Q25); in the call 60 frames later free VMs, scene −1 |

- Cursor (`Menu_CursorUpDown` 0x47030B, wraps, sfx 12; held keys repeat with the repeat flag); locked items 1
  and 2 are skipped and drawn with colour 0xFF404040.
- **Highlight** (VERIFIED 0x4674E0 + `options_cfg` draw log presents 327–366, RE-Q42): "all 9 item VMs get the
  unselected sprite (base + 1), the cursor item its base sprite, then locked items 1/2 get colour 0xFF404040" runs
  only (a) in the entry block of call 0, (b) in sub-state 1 when `Menu_CursorUpDown` returned non-zero (the cursor
  moved; after the locked-item skip loop), and (c) on cancel (old cursor item base + 1, cursor := 8, Quit base
  sprite). Not in calls 1–8, not in sub-states 2 (Quit) and 3 (Option wait), and not on a confirm. The colour is
  never written back to white (an unlocked item keeps its script colour). Consequence: Option's confirm sets cursor
  0 without a highlight, so item 7 stays lit through the wait and the slide-out (the option screen never touches
  VMs 1–9), and back on the top menu call 0 highlights cursor 7. Because the call-0 colour write also sets alpha
  0xFF before the items' label-2 `alpha_fade` starts (start = current alpha), locked items appear at once at full
  alpha instead of fading in.
- When the cursor changes the new description VM gets interrupt 1 (change test, sub-state 1 every call).
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
- **The current description VM is run and drawn on every screen while the pointer +0xC2BC ≠ 0** (menu calc/draw,
  after VMs 0–141, only while quit state +0x14434 = 0), at (320, 448). Leaving the top menu sends it interrupt 2 (fade
  60 frames, then hidden). Complete list of pointer writers (VERIFIED: every access to +0xC2BC, RE-Q37):
  - `MainMenu_LoaderThread` 0x470E10: description VM 0;
  - top menu, sub-state 1, every call: cursor ≠ +0x08 → description VM [cursor], interrupt 1;
  - difficulty screen entry block (0x46A071) and replay screen entry block (0x46E136): **:= 0** — so after Start /
    Extra / Practice / Replay the description is drawn only in the press frame (its interrupt 2 already took one
    step) and is gone from the new screen's first call on;
  - option screen (0x46837B) and key config (0x469636): entry block sets +0x08 := −1; then every call, cursor ≠ +0x08 →
    description VM [cursor], interrupt 1. The option entry call (sub-state 0 → 1 in the same call) renders the 10
    option descriptions (table 0x4C8664) into description VMs 0–9, key config renders its 12 (table 0x4C8690) into
    VMs 0–11, both with `Text_RenderCentered` as for the top menu (COLORREF 0x00FFF0E0 = RGB(224, 240, 255), flags 0x300000). Texts:
    option 0 プレイヤーの初期数を変更します。（初期設定　３）, 1 画面の色数を変更します。３２ＢＩＴだと最も綺麗に表示されます。,
    2 ＢＧＭの再生方法を変更します。（初期設定　ＷＡＶ）, 3 ＢＧＭの音量を調整します, 4 効果音の音量を調整します,
    5 ウィンドウかフルスクリーンか選択します, 6 弾が多い場面でわざと処理落ちさせます(スコア、リプレイ記録不可),
    7 全て初期設定にします, 8 パッド操作のボタン配置を変更します, 9 おいそれと終了します; key config 0 ショット、決定ボタンを設定します,
    1 ボム、キャンセルボタンを設定します, 2 低速移動ボタンを設定します, 3 メッセージスキップボタンを設定します, 4 ポーズボタンを設定します,
    5 上移動ボタンを設定します, 6 下移動ボタンを設定します, 7 左移動ボタンを設定します, 8 右移動ボタンを設定します,
    9 ショット押しっぱなしで低速移動になるようにします, 10 初期設定に戻します, 11 おおよそ終了します.
  - No other handler writes it: the character screens, practice stage select and screens 16/17 keep the old VM
    (after Spell Practice it fades out over its 60 frames on screen 16).
  Consequences: during Option's wait (sub-state 3) and Quit's 60 frames the top description stays drawn unchanged;
  back on the top menu the previous screen's description VM (option / key-config row) stays drawn during the 9
  render calls of sub-state 0, until the change test of call 9 picks VM [cursor] (whose strip the top menu has just
  re-rendered; strips 9–13 keep option / key-config text).
  Exact senders (VERIFIED `MainMenu_Top` 0x4674E0, RE-Q24): the pending interrupt 2 is written into the current description VM (pointer menu+0xC2BC; it is **not** in the title01 range, so range interrupts never reach it) by every confirm that leaves to another screen or scene — Start/Extra/Practice/Spell Practice (with their range interrupt 5/7/…), Replay (range 13), Result viewer (scene 5), Music Room (scene 8) — and by the entry redirects (returning to the replay screen after a replay, range 13; returning to practice / spell practice, range 5). **Option (item 7) and Quit (item 8) send nothing to it**: Option sets sub-state 3 and calls `MainMenu_Option` 0x46837B in place, Quit (sub-state 2) only sends range interrupt 1, so on those the current description keeps its state.
  - **Loader set-up** (`MainMenu_LoaderThread` 0x470E10, only when 0x17CE9E4 = 0; VERIFIED RE-Q42): for i = 0..13,
    description VM i (menu +0xC2C0 + i·0x2A4): +0x204 := text.anm, +0x21A := 9, `AnmFile_StartScript(text.anm,
    VM, script 9)` (AnmVm_Init + one run: sprite 21, pos (320, 448), alpha 0, alpha_time 20 → 255), then
    `AnmVm_SetSprite(VM, VM.sprite + i)` = text.anm sprite **21 + i**. The same loop sets screen-17 info VM i
    (menu +0x11F2C + i·0x2A4) to text.anm sprite 21 + i with its own pos and flags (16 §6). Nothing else ever writes a
    description VM outside its script except: the pending interrupt +0x1FE (1 / 2, by the handlers listed above),
    and `Text_RenderCentered` 0x466650, which on every render reads the font bytes +0x298/+0x299 (0 → 15; StartScript
    leaves them 0) and **sets flag bit 0 (visible)**. No position, colour or other field is written. So a description
    VM hidden by its fade-out (`hide_wait_interrupt` after interrupt 2) is made visible again (alpha still 0, so it
    draws nothing) when its strip is re-rendered (top menu calls 0–8, option / key-config entry).
  - Description strips are text.anm sprites 21+k = the same rows as the screen-17 info VMs. Screen 17 entry overwrites
    strips 0–6.
  - The top menu re-renders all 9 descriptions on each entry.
- **Input timing** (VERIFIED 0x4674E0 and the RE-Q37 run `re/work/runs/q37/play_top`: Up in call 9 moved the
  cursor, Shot in call 18 was ignored, Shot in call 20 started the item). Each entry (+0x0C = 0, +0x1442C = 0):
  calls 0–8 are sub-state 0 and only render description k = +0x1442C (+ the entry block in call 0) and return 1 —
  no cursor, no change test, no demo counter. Call 9: +0x1442C := 0, +0x10 := 0, +0x08 := −1, sub-state 1,
  +0x14430 := 0, and the sub-state-1 code runs in the same call: cursor (`Menu_CursorUpDown`), highlight (only when
  the cursor moved, see "Highlight"), demo counter and the change test from call 9; confirm/cancel only when +0x1442C ≥ 10, i.e. from call 19.
  Confirm: sfx 10 (+ sound flush). Cancel: cursor jumps to Quit (sfx 11).
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
menu's > 3 → 2 store is overwritten, RE-Q25). Extra (12): the entry tests 0x43C317, which always returns 0, so it
always sends range interrupt 12 and sets cursor := 4; then cursor −= 4 (clamped ≥ 0) → always 0; VM 135 gets its base
sprite (VERIFIED 0x46A071, RE-Q37).
Entry block order (call 0, VERIFIED RE-Q37): surface load (unless the previous screen is 5/9/13; a failed load
returns 0), cursor := config +0x21, range interrupt 7/12, clamp, VMs 131–134 sprite + 1 and interrupt 25, VM 131 +
cursor base sprite and 24, then +0x0C := 0, +0x10 := 0 and **+0xC2BC := 0** (§2.2). The +0x6C forward of §2.2 is
tested after this block in every sub-state-0 call: +0x6C ≠ 0 → `Menu_SetScreen(9)` (9 for every difficulty screen),
cursor 0, return 1, skipping the end-of-call counter increments. So the whole entry work is done before the forward.
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
**scene 2** (new game). Cancel (Bomb/ESC edge, VERIFIED 0x46A625, RE-Q37): SE 11 + flush, **g_ShotType := cursor**
(so the next character-screen entry starts on the last highlighted character), then screens 5/9/13 → 4 / 8 (flags
bit 0) / 12 at once (cursor 0, return 1, no interrupt); screen 15 plays SE 11 a second time (same frame: one sound,
13 §1.2), sub-state 3, +0x10 := 0, range interrupt 6, and the call with +0x10 > 29 goes to screen 0 with cursor 2,
flags &= ~1.

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
restarts `bgm/th08_01.mid` in slot 8, details 15 §1.3), +0x27 BGM volume and +0x28 SE volume (0–100; a press steps ±5, holding ±1
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
| +0x8280 | | 13 | advance per glyph in px (×scale x). **Read at draw time** by `Ascii_DrawStrings` (0x402B20) for every queued string — it is not stored in the entry (scale is, entry +0x50/+0x54). A caller that sets it and resets it to 13 before the draw pass has no effect (`Hud_DrawPopups` messages 2/4/5/6: advance stays 13·0.9 = 11.7; VERIFIED RE-Q33) |

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
height) (structs.md AnmSprite).
Timing (VERIFIED RE-Q29): `Frame_Present` 0x442060 calls `ApplyPendingCaptures` right after `IDirect3DDevice8::Present`
of the **same** `Frame_Run` whose calc made the request, so the copy happens after that frame's draw. What the back buffer
then holds depends on the swap effect chosen by `Window_InitD3D` 0x4424C0: windowed → COPY (3); full screen with the
60 Hz refresh → FLIP (2) when the frame-skip byte 0x17CE893 is 0, else COPY_VSYNC (4); the other full-screen variant →
COPY (3). With the COPY effects the back buffer keeps the image just presented, so the capture shows **frame N**
(the frame drawn right after the requesting calc; this is the case of every recorded session, all windowed). With
FLIP it holds the previous buffer, i.e. frame N−1 (INFERRED from the D3D8 swap semantics). A request fails (−1) only if
one is already pending, which cannot happen because every Present applies it.
Capture VMs (VERIFIED RE-Q29): pause menu +0x1A70 and continue menu +0xFE0 both get `capture.anm` (slot 3) **script 0**
via `AnmFile_SetScript`, then pos := (32, 16, 0) written directly; nothing ever interrupts either VM (it is simply no
longer run/drawn when the menu closes). The pause menu draws a stack copy of its VM each frame. Capture option bit
0x17CE8FC bit 1 is set unconditionally by `Window_InitD3D` and never cleared, so it is always set.
`Hud_StageTransition` 0x438F58 (VERIFIED RE-Q34): VM impl+0x3CC0 := `capture.anm` script 1 (`AnmFile_SetScript`),
then `AnmManager_QueueCapture`(3, 32, 16, 384, 448, dst = that VM's sprite rect) — the same rect msg op 9 captures into
(golden: `K screen capture.anm#0 32 16 384 448 128 0`). The return value is ignored. Callers (all three): `GameMgr_Calc`
stage-end handling when `next_scene_after` < 0 (11 §4 step 1: stage 1–5 advance or replay next stage), on the clear
frame; `PauseMenu_Update` "retry" (state 10, practice / spell practice / Extra → scene 11); `ContinueMenu_Update`
"yes" in spell practice or Extra (scene 11). So a normal stage advance captures the playfield **twice**: at msg op 9
and on the clear (confirm) frame (golden presents 10796 / 10918 and 24986 / 25108); the second copy, applied after
that frame's Present, overwrites the first and is what the continuing stage's 168 transition tiles show (§5.7).
VMs: `ascii.anm` scripts 12–21 on 10 menu VMs (title, items "resume / quit / retry", confirm "yes/no" …).
State 0 starts **all ten** scripts (VM i := script 12 + i), sends interrupt 1 to VMs 0–3 only, then clears VM flag bit 0
(`0x406700`, not visible) on VM 8 (script 20, sprite 290 at (344, 428): practice label) when not stage practice, and on
VM 9 (script 21, sprite 289 at (344, 412): slow-mode label) when the gameplay config copy's slow byte (`*(GM+4)`
+0x25, 15 §1; forced 0 for difficulty ≥ 4, practice and spell practice at game start, 11 §3.2) is 0 (VERIFIED 0x4037B0,
RE-Q36). Neither script sets the flag again and the close / quit / retry interrupts go only to visible VMs, so a hidden
VM is never seen; with slow mode on, script 21 runs and shows in normal games.
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
| 4 | closing | after 20 frames: paused = 0, sound command 7 "UnPause" (`0x406C50`; WAV mode: also after a `Music_Stop`, 02 §4 "UnPause after `Music_Stop`", RE-Q48) |
| 9 | quit | after 20 frames: scene 1, paused = 0, `Result_Register(2)` (save) |
| 10 | retry | after 20 frames: spell practice / practice / Extra → music: outside spell practice, or in spell practice when the card's 0x4C7670 record flag +0x10 = 1, `Music_Stop`; else sound command 7 "UnPause" (`0x406C50`, queued) and then `Bgm_StartFadeIn(2.0)` 0x406AC0 (immediate: a 2 s = 120-frame fade-in from −10000, 02 §4 "Fade-in", VERIFIED RE-Q41) — the music is not stopped; then scene 11 (the new GM replays slot 0 only in the `Music_Stop` cases, 11 §4 step 3; VERIFIED 0x4037B0, RE-Q36); else scene 10 |

Shortcut keys at any time: ESC → close (4); Q → quit (9); R → retry (10, not in replay). All confirms play
sfx 10, cursor moves sfx 0 (`se_plst00`).

## 4. Continue / game-over menu (`ContinueMenu_Update` 0x404890) — VERIFIED
Flow and side effects: 11 §4.2. UI: `ascii.anm` scripts 22–24 on three VMs ("continue?", "yes", "no"),
`times.anm` script 1 with sprite = clock counter (the night clock, advanced by one at frame 15 of a continue);
selected item colour 0xFFFF8080, other 0xFF505050; default cursor "No" (state 2) in normal games and Extra; "Yes"
(state 1) only in spell practice when the card was not captured (spell flag bit 9 clear) (VERIFIED 0x404890, corrected Phase 5). Input from frame 30 (state 2) / frame 4 (state 1); UP/DOWN toggle with sfx 0; confirm sfx 10.
**Opening timing** (VERIFIED RE-Q29): game_over 0x164D0BB := 1 is written only by `Player_DyingUpdate` (calc prio 9, real
death with no lives left) and the timeline op of `EclTimeline_Run` (enemy manager, prio 11). The ASCII calc (prio 1)
therefore first runs `ContinueMenu_Update` on the **next** frame. (The same frame's ASCII draw already runs
`ContinueMenu_Draw` with state 0, frame 0, so its "state ≠ 0 or frame > 2" items are not drawn yet.) That first call
does all of state 0 (the early-exit tests, sound, VM setup, capture request, capture VM `SetScript` = one VM step), then
state := 2 (or 1, see above) with frame := 0 and falls through into the state-2/1 body **in the same call**. The capture
VM is run once more at the end of every call, and frame += 1. So state 0 never lasts a whole frame: the first drawn
frame shows the menu VMs and the capture VM at script time 2. (State 0 also has a `frame > 8 → stay` exit that cannot
happen, because frame is 0 on entry.)
**Music** (VERIFIED 0x404890, RE-Q41). Opening (state 0, after the early exits): in spell practice when the card's
BGM-restart flag (`Spell_BgmRestartFlag` 0x439916 on card 0x164D0B8, the 0x4C7670 record flag of 12 §3 state 10) is 0,
`Bgm_StartDip(1.0)` (02 §4: the music **keeps playing** and drops to −10 dB over 60 frames); otherwise sound command 6
"Pause" (WAV mode: the buffer is paused at its position). Confirming "Yes" in spell practice or Extra (difficulty > 3):
not spell practice, or restart flag ≠ 0 → `Music_Stop`; else game_over := 0, command 7 "UnPause" and
`Bgm_StartFadeInFromDip(1.0)` (−10 dB → 0 over 60 frames); then scene 11 and `Hud_StageTransition`. Confirming "Yes"
for a normal continue: command 7 "UnPause" (no fade; the paused music resumes — in WAV mode even when a `Music_Stop` came before, 02 §4 "UnPause after `Music_Stop`").

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
| +0x1C | boss bar alpha 0–255 (i32; state machine §5.1b step 1) |
| +0x20 | boss "remaining bars" count (ECL op 148) |
| +0x24 / +0x28 | boss timer seconds / last value shown. +0x24 is written only by `Enemy_CheckTimeout` 0x42B930 (07 §10.1 step 4, called while timeout ≥ 0): for a boss (flags A bit 1) in boss slot 0 (e+0x3313 = 0) := (timeout e+0x3378 − timer e+0x2E14) / 60 (integer), i.e. with the timer value **before** that frame's step-7 tick; the HUD draws the stored value (VERIFIED RE-Q32, golden 19329: the digit changes one frame after the timer crosses the second). +0x28 is written only by `Hud_DrawBoss` (§5.4). **Lifetime** (VERIFIED RE-Q34: the only other writer of either field is the 0x27-dword (+0x00…+0x9B) zero fill of `Hud_Register` 0x437AD0, which runs from `GameManager_StageStart` only when the fresh-start flag 0x17CE8BC is set; `Hud_Load` writes neither): both are 0 after a fresh start and **keep their values** across a continuing stage (11 §7) and a continue (11 §4.2), so the first guarded `Hud_DrawBoss` of the next stage shows the previous stage's last value until `Enemy_CheckTimeout` writes a new one |
| +0x2C | boss bar requested (byte). Writers (VERIFIED RE-Q34, every `FUN_00422C20(&g_Hud, ·)` call): ECL op 127 slot 0 → 1 (and +0x30 := 1.0), op 127 slot < 0 → 0 when the caller's boss slot e+0x3313 < 4; `Enemy_Free` of a boss (flags A bit 1, slot < 4) → 0; `EnemyMgr_Calc` death: modes 0/1 when flags A bit 1 → 0, mode 3 → 0 unconditionally (07 §10.3); `Hud_Load` → 0. It is a plain request: the bar does not test whether boss slot 0 exists |
| +0x30 / +0x34 | target life fraction / displayed (moves +0.01 up, −0.02 down per frame). +0x30 := f32(life e+0x2DFC) / f32(max life e+0x2E00), no clamp, written by `EnemyMgr_Calc` for a boss (flags A bit 1) in slot 0 only while no dialogue runs (`Msg_IsActive` = 0); ECL op 127 with a = 0 sets it to 1.0 (VERIFIED 0x42C660, RE-Q32). The HUD life e+0x2E04 (op 177 etc.) is never read by the bar |
| +0x3C[8], +0x5C[8], +0x7C[8] | life-bar markers: end, start (f32 fractions of the bar), colour (ECL op 158: word 1 / e+0x2E00 → start +0x5C, word 2 / e+0x2E00 → end +0x3C, word 3 → colour; e = the enemy running the op, its max life at op time; VERIFIED 0x4230E0 / 0x423110, RE-Q32) |

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

### 5.1a `Hud_Load` 0x4390EE — fresh start vs continuing stage (VERIFIED RE-Q32)
Branch on the fresh-start flag 0x17CE8BC (11 §3):
- **Continuing stage** (flag 0, i.e. stage 2+ of a run, 11 §7): the impl block is **not** cleared and front.anm,
  times.anm and loadingNN.anm are **not** reloaded. impl+0x398C / +0x3C30 / +0x3ED4 (script-index shorts of the VMs
  impl+0x3778, +0x3A1C, +0x3CC0) := −1, impl+0x21810 := 0; then VM impl+0x3CC0 := `capture.anm` script 1 with its
  +0x214 short := 1, the 168 stage-start transition VMs impl+0x5728 + (r·12 + c)·0x2A4 (r = 0..13, c = 0..11) :=
  `capture.anm` script 3 + ((r + c) & 1), their I4 (+0x120, var 10008; 05 §2) := r + 2c, pos (+0x208) := (32c − 0.5 + 16, 32r − 0.5 +
  16, 0), uv offset (+0x30, +0x34) := (32c/512, 32r/512); impl+0x21810 := 168 (0xA8).
- **Fresh start**: the whole impl block (0x8C2E dwords) is zeroed; front.anm (slot 10), times.anm (14) and the
  shot's loadingNN.anm (12) are loaded; the same three shorts := −1 and +0x21810 := 0; VM impl+0x5484 := `ascii.anm`
  script 0x1A, VM impl+0x22E14 := `ascii.anm` script 0x19 with its difficulty sprite (§5.1 table).
- Both: night clock hidden; clock VM impl+0x34D4 := `times.anm` script 0, sprite = clock counter; msg file (not in
  spell practice); stgNtxt.anm (unless the reload-same-stage flag 0x17CE8C4); **only on a fresh start** the 16
  front VMs impl + i·0x2A4 := front.anm script i (so on a continuing stage VMs 0–15 keep their state: VM 13 has
  stopped, the panel VMs 0–9 stay fully faded in); HUD +0 := 0, +0x2C := 0, impl+0x2A40 := 0, +0x30 := +0x34 := 0;
  stage-title VMs (stgNtxt scripts 0–3, or script 3 only in spell practice); impl+0x2181C := −1, +0x22D84 := 0,
  popup timers +0x22D9C / +0x22DBC / +0x22DDC := 0; all six dirty fields := 2; stage-clear VM impl+0x212C8 :=
  `ascii.anm` script 3; 0x17CE8CC := 0x10; impl+0x22E0C := 0.

### 5.1b Per-frame order (`Hud_Calc` 0x4338CA / `Hud_Update` 0x435900) — VERIFIED (RE-Q29)
`Hud_Calc` does nothing while the freeze byte 0x160F534 (GM+0x2C) is set. Otherwise, in this order:
1. `Hud_Update`:
   1. boss bar (only while no dialogue script runs, impl+0x2181C < 0). State byte impl+0x2A40 (0 off, 1 fading in,
      2 shown, 3 fading out); L = "Enemy" label VM impl+0x1FB0 (front.anm script 12); W = L's flag bit 14 (set while L
      waits in `wait_interrupt` / `hide_wait_interrupt`, cleared when L takes an interrupt in its run, 05 op 20).
      This step runs **before** L is run in step 2, so W is the value left by L's run of the previous frame
      (VERIFIED 0x435900, RE-Q34):
      - +0x2C = 0: if state ≠ 0: { if state < 3: interrupt L 2, state := 3. If alpha ≠ 0: alpha −= 4. If W: state := 0,
        displayed +0x34 := 0, alpha := 0 }. So hiding a fully shown bar (L waiting at its t 30, W = 1) ends at once:
        nothing of the guarded part is drawn from that frame on (no fade-out; golden presents 4432–4494 = game frames
        3697–3759 after the stage-1 boss, while L's own 30-frame fade-out runs undrawn). Hiding during the fade-in
        (W = 0) fades alpha down by 4 until L reaches its `hide_wait_interrupt` 30 frames later.
      - +0x2C ≠ 0: state 0 → interrupt L 1, state := 1, **alpha := 0** (the request frame draws the bar with alpha 0,
        i.e. nothing: 05 §9 alpha-0 rule; golden 3671). Else: if W: state := 2; alpha := alpha + 4 if alpha < 0xFC
        (unsigned) else 0xFF.
      - Then, if state ≥ 2 (never in states 0/1, so the fill stays at 0 during the 30-frame fade-in): displayed +0x34
        moves to target +0x30: += 0.01 (f32 0x4B48D4) capped at the target if below it, −= 0.02 (0x4B4950) floored at
        the target if above it.
   2. run front VMs (16), stage-title VMs (4), the stage-start clock impl+0x34D4 (§5.1 condition), the stage-clear VM
      impl+0x212C8, the night clock impl+0x2156C;
   3. night-clock player-overlap fade (§5.7);
   4. run VMs impl+0x5484 and impl+0x22E14; while impl+0x398C ≥ 0 the spell-name VMs impl+0x3778 / impl+0x3CC0 and the
      8 VMs at impl+0x3F64; the 168 stage-start transition VMs while impl+0x21810 ≠ 0;
   5. popup timers impl+0x22D9C / +0x22DBC / +0x22DDC (§5.5);
   6. **clear bonus** when impl+0x22D84 = 1 (§5.6), then impl+0x22D84 := 2;
   7. **clear trigger** test, then the **clock animation** step (§5.6).
2. `Msg_Run` (10 §5): the dialogue interpreter, including op 9, which sets impl+0x22D84 := 1 and flags bit 9.
3. If the raw input has SKIP (0x100) and 0x17CE8CC < 8: 0x17CE8CC := 8 (§4).
4. HUD frame counter +0 += 1.

Because `Hud_Update` runs **before** `Msg_Run`, whatever a msg op sets this frame is acted on by `Hud_Update` the
next frame. In particular, the clear bonus of op 9 is added to the score on the frame **after** op 9 (and after
flags bit 9 appears). The trigger and animation also start one frame later.

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
script (ip ≠ NULL)** — front.anm script 13 ends with `stop` at time 200, so the first 200 HUD run frames after a
**fresh-start** `Hud_Load` always repaint (golden: 199 consecutive full-repaint frames from the stage-1 start; on a
continuing stage VM 13 is not restarted, §5.1 `Hud_Load`, so only the counter repaints: 61 frames at the stage-2 start)
— or the counter 0x17CE8CC ≠ 0. The full repaint draws VMs 0 (snap), 1 (rot), 2–9 (snap) and the difficulty VM
impl+0x22E14 (snap), then sets all six dirty fields to 2.
The original only repaints changed rows (dirty counters / full repaint counter 0x17CE8CC, writers below; the back
buffer is kept between frames, see `specs/data/drawlog/FORMAT.md` §4). A port that wants draw-log parity must follow
these rules exactly and keep its back buffer (present by copy, clear only what the original clears); a port that does
not care may redraw everything every frame. Full repaint loop bounds (all `<`, step 32 unless noted, VERIFIED
0x43625D, RE-Q32): tile 13 at x = 0 for y = 0, 32, … < 464 (0..448); tile 13 for x = 416, 448, … < 624 (416..592) ×
y = 16, 48, … < 464 (16..432); strip 14 (VM impl+0x24F8) at y 0 and y 464 for x = 0, 128, … < 624 (0..512); then the
VMs. Each frame strip 14 is also drawn along y = 464 for x = 32, 160, 288 (x < 368).

**Per-frame order of `Hud_DrawPanel`** (VERIFIED 0x43625D, RE-Q32; `dirty(f)` = the 2-bit field f of HUD +0x04 ≠ 0,
`cfg4` = cfg+0x38 bit 4):
1. flush; viewport (0, 0, 640, 480);
2. unless cfg4: blank patches (480, 40), (480, 56), the dirty-row patches, (512, 464) (above);
3. full repaint (above) if its condition holds — it sets all six fields to 2, so everything below draws;
4. if dirty(lives): life icons; if dirty(bombs): bomb icons; if (dirty(bombs) or dirty(lives)) and GameFlags bits
   7–8 = 1 and a spell card is active (g_Gui +0 bit 0): `Draw2DSnap` VM impl+0x5484 (`ascii.anm` script 0x1A);
5. strip 14 along y = 464 (x 32, 160, 288);
6. ASCII strings, queued in this order: **score row y 56** (`%.9d` displayed score, then x +117 = 605 continues
   used), **hiscore row y 40** (`%.9d` hiscore, x 605 hiscore continues), scale reset to 1; then, each only if
   its field is dirty or cfg4: graze (y 152), point items (y 168: value, "/" at 0.5 × scale, value), time (y 184,
   whole row 0xFFFFF0C0 when time ≥ threshold, colour reset to white after);
7. flush; if dirty(power) or cfg4: the gauge quad **only when power > 0**, then the power text ("MAX" at ≥ 128,
   else `%d`);
8. every non-zero dirty field −1 (order lives, power, bombs, graze, point, time).
So the score/hiscore rows are queued every frame, the other rows only for 2 frames after a change (a field set to 2
is drawn in two consecutive frames = both back buffers).

**Dirty-field writers** (HUD +0x04 = 0x160F42C; "→ 2" = field := 2; VERIFIED RE-Q32, every direct reference in
globals_xref plus `Hud_Load`):
| writer | fields |
|---|---|
| `Hud_Load` (every stage) | all six → 2 |
| `Hud_DrawPanel` full repaint | all six → 2 |
| `Spell_Start` 0x4152A0, `Spell_End` 0x4161B0 | +0x04 \|= 0xF (lives and bombs fields := 3) |
| `GameMgr_Extend` 0x439B29 | lives → 2 (life added), else bombs → 2 (bomb added) |
| msg op 9 (10 §9, stages 1–5, shots 3/10/11 below 3 bombs) | bombs → 2 |
| `ItemMgr_Calc` bomb item (bombs < 8 only) | bombs → 2 |
| `ItemMgr_Calc` full-power item (always) | power → 2 |
| `Item_CollectPowerSmall` / `Item_CollectPowerBig` (only when power < 128) | power → 2 |
| `Item_CollectPoint` 0x440E40 | point → 2 |
| `Item_CollectTimeOrb` 0x4412B0 | time → 2 |
| `Player_OnGraze` 0x44A930 | graze → 2 (also while bombing, when the graze count is not raised) |
| `Player_UpdateBomb` bomb start | bombs → 2 |
| `Player_UpdateBomb` while a bomb runs (+0xFDC ≠ 0): bomb timer +0xFF4 changed (current ≠ previous) | point → 2 (QUIRK: the point row, every bomb frame) |
| `Player_DyingUpdate` respawn with lives > 0 | lives → 2, bombs → 2 |
| `Player_DyingUpdate` real death (deathbomb window over) | time → 2, power → 2 |

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
- 0x10: `Hud_Load`; 0x3C: `GameManager_StageStart` (after the loader wait, with `_g_GmLoading` := 0); 0x1BA: `Msg_Run` op 14 (10 §5.1, the white flash); 
- 0: `Supervisor_Init`.
A port that redraws the whole screen every frame can ignore it.

### 5.4 Boss UI (`Hud_DrawBoss` 0x43741D; the bar part only when no dialogue runs and the bar is shown or fading) — VERIFIED
`Hud_DrawBoss` order (VERIFIED RE-Q29): **always**: the 4 stage-title VMs, the stage-start clock impl+0x34D4 and the night
clock impl+0x2156C (`Draw2DRot`); while impl+0x398C ≥ 0 the spell-name VMs (impl+0x3778 `Draw2DSnap`, impl+0x3CC0 and the
8 VMs at impl+0x3F64 `Draw3DQuad`, and, while impl+0x3C30 ≥ 0, impl+0x3A1C with its pos +0x208 := (304, 448, 0) first, `Draw2DSnap`); the 168
stage-start transition VMs while impl+0x21810 ≠ 0. **Guarded** (no dialogue script, impl+0x2181C < 0, and bar requested
+0x2C + bar state impl+0x2A40 ≠ 0): life bar, markers, "Enemy" label VM impl+0x1FB0, remaining-bars gauge, timer, and the
chain-count popup below. **Always, last**: the stage-clear VM impl+0x212C8 (`Draw2DSnap`).
- All bar quads (life bar, markers, remaining-bars gauge) are `Screen_FillRect3` 0x45B490 untextured quads (z 0, rhw 1,
  vertex 0–1 = first colour, 2–3 = second) at the **exact** coordinates below — no −0.5 pixel shift and no snapping
  (golden: `Q 64 19 …`, VERIFIED RE-Q32).
- Life bar quad x 64 … 64 + 320·displayed_fraction, y 19..23, colour alpha|0xFFFFFF (top) / alpha|0x202060.
- Markers k (8 slots, enabled when end ≠ 0 and start < displayed): x 64 + 320·start … 64 + 320·min(end,
  displayed), colour alpha|colour, bottom alpha|((colour >> 2) & 0x3F3F3F).
- "Enemy" label VM 12 at (80, 26).
- Remaining-bars gauge: `n = +0x20` segments between x 35 and 61 (segment i: 35 + 26·i/n … 35 + 26·(i+1)/n −
  gap, gap 2 if n < 6 else 1), y 19..23, top colour alpha|(0xFFFFFF − i·255/9), bottom alpha|0x202020 (VERIFIED RE-Q32).
- Timer at (384, 16): drawn on **every** guarded `Hud_DrawBoss` call, with no further condition (no boss / timeout test;
  VERIFIED 0x43741D RE-Q34, golden 3690–3760: the reference draws it during the bar fade-in). Value v = min(+0x24, 99)
  (clamped, not skipped: +0x24 ≥ 100 shows "99"), printed with `%.2d` (two digits; a negative +0x24 would print
  "-01" etc.). Colour from the **unclamped** +0x24: ≥ 20 0xA0D0FF, ≥ 10 0xA080FF, ≥ 5 0xE080C0, < 5 0xFF4040 (RGB,
  alpha = bar alpha). Sound: if +0x28 ≠ +0x24 (raw values): v < 3 sfx 0x26 `se_timeout2`, else v < 10 sfx 0x1D
  `se_timeout`; then +0x28 := +0x24 (after the print). +0x28 is 0 after a fresh start (§5.1), so a first value < 10
  plays the sound. The glyphs are `Draw2DSnap` VMs, so with bar alpha 0 nothing is drawn (05 §9 alpha ≠ 0 rule;
  golden 3671 / 6097–6098: the life-bar quad is drawn with alpha 0, the timer is not).
- The guard reads +0x2C **live** at draw time: when the request is set after that frame's `Hud_Calc`, the guarded part
  is drawn in state 0 with the old alpha 0 and `Hud_Update` starts the fade-in on the next frame (golden 6097: two
  alpha-0 frames before alpha 4) (VERIFIED RE-Q34, scratch fix E).
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
Timing (VERIFIED `Hud_Update` 0x435900, RE-Q33): the show call sets x = 416 and the timer t := 0. Each `Hud_Update`:
if t < 30: x := f32(t·(−312)/30 + 416) with t the timer **before** this frame's tick, else x := 104; then if t ≥ 250
(bonus) / 180 (message) / 280 (spell bonus) the popup is switched off; then the tick. A popup shown before the
same frame's `Hud_Update` is therefore drawn at 416 on its first frame, at 405.6 on the second, and drawn for
250/180/280 frames. (RE-Q33 golden: a port one tick ahead drew the rows 10.4 px left for 30 frames.)
| `Hud_ShowMessage(v, type)` 0x437E5D | Callers (VERIFIED RE-Q34, byte scan of every `call` rel32 / absolute reference: exactly 4 sites): `Item_CollectPowerSmall` 0x440B0F and `Item_CollectPowerBig` 0x440D91 (type 1, 08 §8.5), `ItemMgr_Calc` 0x44120A (type 1, full-power item), `Spell_End` 0x416823 (type 5/6, 07 §11.1); no code writes the type field +0x22DBC except these, `Hud_Load` and the expiry, so **types 2, 3 and 4 are never shown** (unused text). 1 "Full Power Mode!" (power items at max) and 3 "CherryPoint Max!": colour 0xFFC0B0FF, scale 1; 2 "Supernatural Border!!", 4 "Border Bonus %7d", 5 "Spell Bonus Failed", 6 "Last Spell Failed": scale **x 0.9, y 1.0**, colour **0xFFE0B0FF**, `Ascii_SetAdvance(11)` that is reset to 13 before the draw, so the effective advance is 13·0.9 (§2.9) (VERIFIED 0x438A89, RE-Q33) | (416, 168) sliding like above; 180 frames |
| `Hud_ShowSpellBonus(n)` 0x437EDC (capture) | "Spell Card Bonus!" red 0xFFFF0000 at x = (384 − 17·14)/2 + 32 = 105, y 80 (scale 1, advance 13); then "+%d" at y 96, x = (384 − len·28)/2 + 32, 2× scale, colour 0xFFFF8080 (VERIFIED 0x438A89, RE-Q25) | 280 frames |

### 5.6 Stage-clear panel (`Hud_DrawClearPanel` 0x43826B, while msg op 9 state is set) — VERIFIED (rewritten RE-Q25)
`X` = "a replay of a practice game" = `0x43C322`: GM flags bit 3 (replay) **and** info byte +0x7B of the loaded replay
(15 §3.5, the practice flag recorded at record start). Replay playback never sets GM flag bit 0 (15 §3.8), so practice
replays are recognised only through +0x7B. `P` = GM flag bit 0 (practice).

**Bonus computation** (`Hud_Update`, once, on the first frame after op 9, §5.1b; all integer arithmetic):
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
| title | stage < 6: "Stage Clear"; else "All Clear!" — for stage idx > 6 (6B, Extra) the pen y is lowered by 16 **before** the title and every later line keeps that offset, i.e. the whole panel starts at y 80 ("Clear" at 112) (VERIFIED 0x43826B, RE-Q29) | | 0xFFFFFF40 |
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

Draw-time values (VERIFIED 0x43826B, RE-Q29): Clear / Point / Graze / Time / Total and the clock minutes come from the
values stored by op 9 and the bonus step (impl+0x22DEC…, +0x22D88, +0x22E04/08/0C), but the "Player" / "Bomb" lines and
the 6B "Night Bonus" amount call `GameMgr_GetLives` / `GetBombs` / `GetClock` **every draw** (live values; "Last Time"
uses the stored +0x22E08). The percentage lines divide by 0x164D31C with `idiv` and no zero test: f = 0 would raise an
integer divide fault (no handler; the game would crash). Retail data never reaches it (every op 9 follows counted
gameplay frames), so a port may treat f = 0 as unreachable. impl+0x22D88 (Total) is written only by the bonus step; the
HUD impl block is reallocated and zero-filled only on a fresh start (`Hud_Register` / `Hud_Load`), so on a continuing
stage the previous stage's total stays until the next op 9 (impl+0x22D84 is reset to 0 by `Hud_Load` every stage).
So the lines shown and the bonus actually added differ: practice replays add the lives/bombs bonus without showing
it, practice games and practice replays add the 6B night bonus without showing it, and the lives-option penalty is
applied in every game but shown only below Extra outside practice.
Clear trigger and clock animation (VERIFIED 0x4360F7–0x43625B, RE-Q29). `shown` = impl+0x22E0C (op 9 sets it to the
old minutes, always ≥ 660, so ≠ 0; 0 before any op 9), `target` = impl+0x22E08, `timer` = impl+0x22E10 (op 9 := 0).
Each `Hud_Update`, after the bonus step, in this order:
1. **Trigger**: if stage idx < 6, `shown` ≠ 0, `shown` ≥ `target` and GM flags bits 5–6 = 0 → bits 5–6 := 2 (the
   stage-clear trigger, 11 §4). There is **no** 60-frame gate here: when the clock did not advance (`shown` =
   `target` from op 9 on), the trigger fires on the first `Hud_Update` after op 9 (the same frame as the bonus).
2. **Animation**, only if `shown` ≠ 0 and `shown` ≠ `target`: if `timer` < 60 → `timer` += 1; else if `shown` <
   `target` → `shown` += 1, then += 3 more if the game input (g_Input 0x164D52C) has SHOT (1) or SKIP (0x100),
   then clamp to `target`; else (`shown` > `target`) `timer` += 1.

Because the trigger is tested before the step, it fires on the frame **after** the step that makes `shown` reach
`target`.
Counters for the percentage lines: 0x164D31C counts this stage's non-dialogue player frames, 0x164D32C / 0x164D328 the
frames at human / youkai max gauge; reset at stage start, incremented in `Player_Calc` together with **+100 points per
frame** at either max.

### 5.7 Other HUD elements
- Stage-start transition: 168 VMs (14 × 12 grid of 32-px `capture.anm` tiles, script 3/4 alternating,
  staggered by `row + 2·col`) drawn as 3D quads while active. impl+0x21810 (VERIFIED RE-Q34: written only by
  `Hud_Load` (§5.1a: 0 / 168) and `Hud_Update`): while it is ≠ 0, `Hud_Update` runs **all** 168 VMs and then stores
  168 − (number of `AnmVm_Run` calls that returned 1); `AnmVm_Run` returns 1 on the frame a script ends **and** on every
  later call of a VM with no script (+0x220 = 0), so the value is the number of tiles still running, recomputed every
  frame (a VM paused by flag bit 19 returns 0 and counts as live). At 0 the tiles are neither run nor drawn
  (`Hud_DrawBoss` §5.4). The tiles sample the `capture.anm` texture rect of script 1, i.e. the **latest** capture into it:
  on a normal stage advance that is the `Hud_StageTransition` capture of the clear frame (§3 capture callers), not the
  msg op 9 one.
- Night clock VM impl+0x2156C (`times.anm`, VERIFIED RE-Q25): ECL op 179 (0x439007) starts script 2 on it with
  sprite = clock counter; op 181 sets sprite = clock and interrupt 1 (2 when the clock reaches 12) (07 §7); op 180 and
  `Hud_Load` (0x4390D6) set its alpha byte to 0. It is run every `Hud_Update` (after the stage-clear VM) and drawn
  every frame by `Hud_DrawBoss` (`Draw2DRot`, playfield viewport), right after the stage-title VMs and the clock
  impl+0x34D4.
- Player-overlap fade (VERIFIED, `Hud_Calc`): impl byte +0x2175F is the **alpha byte of the night-clock VM**
  (impl+0x2156C, `times.anm` script 2, ECL ops 179/181; op 180 = 0x4390D6 sets it to 0 = hidden). While the
  alpha is ≠ 0: if the player is at x ≥ 64 and y < 128 it drops by 4 per frame down to 64 (0x40); otherwise
  it rises by 4 per frame up to 255 (values ≥ 0xFC snap to 0xFF; 0xFF stays).
  Order (VERIFIED RE-Q29, §5.1b): the fade runs **after** `Hud_Update` has run the night-clock VM in the same call, so a
  script alpha change made by the VM this frame is then adjusted by the fade. The player position is the player object's
  pos (0x17D61AC/0x17D61B0), i.e. playfield coordinates (0..384, 0..448), compared against x 64 / y 128.
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
(+0x298/+0x299) to 15 (the copies 8/9 keep 0, because their scripts are started with `AnmFile_SetScript`, which does not touch these
bytes, unlike `AnmManager_SetScript` 0x40B910, which sets both to 15). No visible effect: `Text_Render` 0x466310
replaces **any** size ≤ 0 (width and height) with 15 (VERIFIED RE-Q26), and the right-aligned renderer also uses 15
for a 0 width.

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
unless spell flag bit 10 (name only); SE 0xE. **Boss hide** `Spell_HideName` 0x416160 (native 23, `Spell_End` 07 §11.1, `Spell_Calc` step 1.1): VM 7 pending
interrupt 1, VM 11 interrupt 2, VM 13 interrupt 2 (unless bit 10). Nothing else interrupts VM 7, so the boss
name stays parked at its top position until native 23 or the next show.

Calc: all VMs (0, 2, 4, 6, 10, 1, 3, 5, 7, 8, 9, 11, 13 in this order) run only when bullets are not frozen
(GameFlags bit 10) and the freeze byte 0x160F534 is 0. They run **after** the card-active step or the capture-ring step
of the same `Spell_Calc` (07 §11.1 steps 1–2; the ring/orb step runs whenever the card is inactive, including after
native 22's name-only declaration, which clears bit 0), and are followed by the decrement of the remaining-time timer
+0x108 (VERIFIED RE-Q27). The tint +0x2620 used by draw step 4 starts at 0x80808080 at every `Spell_Start`; while a
card is active its alpha byte +0x2623 moves by 4 per frame between 0x20 and 0x80: down while the player is at
x ≥ 64 and y < 64 (playfield coordinates), up otherwise (07 §11.1 step 1.2) — the boss name, plate, label and bonus
line fade to alpha 0x20 when the player is under them.
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
   0x88 + d (`ascii.anm` 8×12 digits), x += 7 after **every** one of the 8 slots (suppressed ones too); then
   x += 32: captures of this card (+8) for the current shot type (CATK story +0x158 / practice +0x1F4, capped
   999) as 3 slots — hundreds (drawn if ≠ 0), x += 7, tens (drawn if ≠ 0 or hundreds drawn), x += 7, units
   (always) — **no advance after the units**; then x += 13: attempts (CATK +0x124 / +0x1C0, capped 999) the
   same way, except that its tens digit is also forced when the captures had a hundreds digit. So the attempts
   hundreds slot is 13 px (not 20) right of the captures units (VERIFIED 0x4178C0, RE-Q33: golden y 334.5/338.5
   attempts digit at x 392.5, a port with a trailing advance draws it at 399.5). Finally the global tint is turned off.

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
Interrupts (`0x422BB0`): op 127 slot ≥ 0 → 1 to M_slot; op 127 −1 and `Enemy_Free` of a boss → 2, and
M_slot pos := (−999, −999, 0). `Enemy_Free` 0x42BCF0 tests **flags A bit 1 (boss) and** the slot byte e+0x3313 < 4; op 127
−1 clears bit 1 (but not the byte), so a later `Enemy_Free` of that enemy does nothing here. e+0x3313 is written only by
op 127 (0x41C562, also with −1: the byte keeps the old slot); the template value is 0 (VERIFIED RE-Q27). A boss that dies
with death mode 0 (07 §10.3) keeps its table entry and bit 1 and is never freed: its death frame still runs the
per-frame marker step below, later frames skip the dead slot, so **its marker stays visible at its last position with
its last mode** until op 127 −1 / a new op 127 on that slot / the next stage's `Ascii_SetupGaugeVms` (VERIFIED 0x42C6A0). `Player_Load` → 2 to M_0, M_1, M_2; player on-remove with the full-exit flag → 99 (→ hidden)
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
  % 2 == 0 (counter rule below): sprite 158 (0x9E, the 48×16 plate at (48, 240)) and colour 0xFFFFFFFF; on the other frames as mode 0.
**Frame counter +0x8284** (VERIFIED RE-Q27, displacement scan + runtime): its only writer is the `+= 1` at the very end of
`Ascii_Calc` 0x402200 (after the early-out branches, so it also counts paused and game-over frames); it is never reset
(`Ascii_InitVms` zero-fills +0x2264..+0x8263 and +0x82A0.., not +0x8284; the manager is a static object, so it starts at
0). The ASCII manager is registered (calc prio 1) by `Supervisor_Init` 0x4464C8 on the **boot loader thread**, after
`text.anm` is loaded — not when the supervisor registers. So the calc passes run while the boot loader is still working
(the 13 "boot calc" rows of every recorded session: `release boot calc 12` in events.txt) do not call `Ascii_Calc`, and the
counter equals the main-menu-relative frame number: 1 after the calc of the first menu frame, N after menu frame N, through
menus, loading screens, pauses and gameplay (golden: counter == poll on every row checked up to 4400). A port that counts
from its own boot registration is 13 ahead (13 mod 4 = 1: the mode-3 blink one frame early).
Then `Draw2DSnap(M_k)` (still subject to the VM's visible bit and alpha ≠ 0). The colour bytes are overwritten by the
draw every frame, so after the 30-frame fade-in the script alpha no longer matters. Golden draw log: `ascii.anm#0`
quad (199.5, 463.5)–(247.5, 479.5), uv (0, 0.9375)–(0.1875, 1), colour 0x60FFFFFF / 0x80FF4040 during the stage-1 boss.

## 6. Answers to deferred implementer questions
- Main menu scripts, highlighting and interrupts: §2.1–2.2 (all 142 `title01.anm` scripts are started at once;
  highlight = base sprite vs base+1; interrupts listed).
- Where/when `th08logo.jpg` is shown: §1 (surface slot 8 during boot loading until the title is ready).
- Op 59 with n = 0 does not draw from the RNG; RNG algorithm/seed: 11 §1.
