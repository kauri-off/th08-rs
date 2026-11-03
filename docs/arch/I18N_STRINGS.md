# I18N_STRINGS — inventory of every displayed string (run G0 of I18N.md §4)

RE / docs run, 2026-11-04. Read-only: specs 00–16 + structs.md, port source (`crates/th08-core`, `th08-render`), `re/`
dumps (`re/extracted/dat`, `re/tools/msgdump.py`), th08.exe 1.00d (string reads only) and the **local thcrap install**
`~/Games/Touhou Collection/TH08 - Imperishable Night/TH08 - Imperishable Night/thcrap/repos/` (`nmlgc/base_tsa`,
`nmlgc/script_latin`, `thpatch/lang_en`; **no `lang_ru` locally**). Below, `<thcrap>` = that `repos/` dir.
Open points: `I18N_QUESTIONS.md`.

Line numbers are for commit 7a218b6. Port paths are relative to `crates/th08-core/src/` unless written in full.

## 0. Results in one screen

1. **Nothing text-derived feeds logic or RNG** (§4). Every pixel width, glyph count, ascii position and text-strip x
   is computed only for drawing. String **content** does reach logic or persistent files, but always as
   **logic-key bytes** (ECL card name/owner/comments, msg/end/musiccmt bytes, the player's typed name). The
   cases are: the CATK checksum over the card name, replay/score.dat copies of the name, ending empty-line handling,
   and the music-room empty-comment flag. A presentation pack never changes these bytes. The design in I18N §2.1 holds,
   with one rule for G1: **translate at the renderer, never in the queued world text** (`Dialogue.texts`,
   `Gui.texts` and the ascii queue hold raw logic bytes; §4.3).
2. **msg `h1` key = msg op 8 `boss_card`**. In thcrap's `msg06` format, op 8 is a `hard_line` typed `"h1"`. th08 uses
   `msg08`, which has **no op 8** (`<thcrap>/nmlgc/base_tsa/formats.js`), and 10 §5.1 says op 8's arguments,
   including the string, are **ignored**. The boss name card is the texture `face_stNN_name.png`. The 7 `h1` entries in
   lang_en are dead data. G3 drops them, and the translated name comes from the image override (§3.1).
3. The thcrap msg key rule was reconstructed and checked on all 32 EN files: **1535 / 1535 keys reproduced**, 0
   misses (§3.1). Ending keys = 0-based file line of the first line of a text block: **367 / 367** (§3.2).
4. More recording sites than "six": the six `TextCmd` **recorders** are right, but result.rs reaches the
   renderer through `menulist::render_strip`, and each recorder has several callers (§1).
5. Mapping traps for G3: `themes.js` keys are **music-room entry + 1**, not the bgm file name (`th08_02` = `th08_00.mid`).
   stringlocs "Spell Condition … 205" labels are **1-based card numbers** (= card id 204). msg entries that share an
   offset are translated only under the lowest index (§3).

## 1. Text-drawing sites in the port

### 1.1 GDI text into `@` textures (`ResCmd::Text(TextCmd)`, 10 §6)

| recorder (file:line) | original entry point | target strip | x rule (vanilla) | callers (file:line) → content |
|---|---|---|---|---|
| `hud.rs:499` `flush_texts` | `Msg_Run` → `Text_RenderToVm` 0x4663B0 | text.anm sprites 0/1 (and 2 = boss_card strip, 10 §5.1 op 16 quirk) | 0 | `msg.rs:342` `render` (ops 3/16/19/20), `msg.rs:348` `clear_line1` (`" "`) |
| `gui.rs:276` `flush_texts` | `Gui_ShowPlayerSpellName` 0x415D60 (ToVm) / `Gui_ShowBossSpellName` 0x415F00 (`ToVmRight`) | text.anm sprite 4 (player) / 5 (boss) | player 0; boss `trunc(320 − len·15/2)` (`gui.rs:235`) | `gui.rs:199` bomb name; `gui.rs:236` boss card name |
| `ending.rs:436` `render_vm` | `Ending_RunScript` 0x428B80 (ToVm, outline −1) | text.anm sprite of VM[line] | 0 | `ending.rs:386` (.end text line) |
| `musicroom.rs:129` `render_vm` | `MusicRoom_Init` 0x44970F / `MusicRoom_StateInput` 0x448B16 | music00.anm `@` row strips; text.anm scripts 10–16 (comments), 0x11 (title) | u0 (always 0) | `musicroom.rs:333` title list; `:235` comment line k; `:282` "now playing" title |
| `menulist.rs:517` `render_strip` | `Text_RenderToVm` (spell practice, replay, result) | text.anm 21+k, resulttext.anm 2+k | 0 | `menulist.rs:698/704` screen-16 headers; `:959` card rows; `:1052` info lines; `:1085` screen-17 header; `:1372` replay card name; `result.rs:272` (`Result::render`, callers `result.rs:584/590/644/666`) |
| `menu.rs:455` `render_description` | `Text_RenderCentered` 0x466650 | text.anm sprite 21 + i | `trunc(256 − len·15/4)` (`menu.rs:260`) | `menu.rs:571` top menu; `:958` option; `:1098` key config |

`len` is the **Shift-JIS byte length** in both x formulas (10 §6). x is computed in core: `gui.rs:235` stores it in the
world queue `Gui.texts`, and `menu.rs` passes it in `TextCmd.x`. For translated text, G1/G2 must recompute x from the
measured width on the render side (§4.3).

The original has a 7th caller: `Result_NameEntry` renders the shot title (table 0x4C7F4C) with `Text_RenderCentered` into
result text VM 0 (16 §2.2). The port does not record it. It is never visible, because text VM 0 is drawn only in
states 8 / 0x13–0x15, and those re-render it first (16 §2.4). It is not in the inventory.

### 1.2 ASCII-sheet text (ascii.anm glyph sprites, 12 §2.9)

Queued `ascii::Text` items (`ascii.rs:21`) are drawn by `emit_texts` (`ascii.rs:139`). Each byte c is drawn as the
sprite c − 1 of `ascii.png`, or c + 0x8A of `asciis.png` when highlighted. The pen advances 13 px × scale. `'\n'` adds
16 px. **Only bytes < 0x80 are text** (0x7F = ▶ cursor, 0x80/0x81 = SP/END, sprite 0x88+ = digits). So these strings
are texture text. A Cyrillic or other non-ASCII translation of them needs a glyph-sheet remap (`I18N_QUESTIONS.md` Q3).

### 1.3 Textures with baked text

Text drawn as ANM sprites (title/menu labels, stage titles, boss name cards, pause/continue menu, result labels,
staff roll), plus `.jpg` pictures. They go through `PresentView` image overrides keyed by the ANM chunk name (§2.11).

## 2. String inventory

Columns: **port site** = where the string is produced (file:line) → recorder of §1. **Source** kinds: `msg op N` (msg
file bytes), `ECL op 122 +off` (card record, 07 §11), `.end line`, `musiccmt`, `score.dat CATK` (bytes first copied
from ECL), `exe` (literal at VA; formats are printf), `typed` (player input), `tex` (baked in a texture).
**thcrap id** = `stringlocs` id (`<thcrap>/nmlgc/base_tsa/th08/stringlocs.v1.00d.js`, key `Rx<RVA>`, VA = RVA +
0x400000) and the `stringdefs.js` text, or the patch file key. "—" = thcrap has none.

### 2.1 Dialogue (10 §5–§6)

| string | port site | source | spec | thcrap id |
|---|---|---|---|---|
| dialogue line, 1 box = 1–3 ops | `msg.rs:342` → `hud.rs:499` | msg op 16 `str` (2214 uses), op 3 `str` (18), op 19/20 `str` (4 each, Final A/B choice lines) | 10 §5.1, §5.4, §6 | `msgNx.dat.jdiff` → `{entry: {"<t>_<k>": {lines}}}` (§3.1) |
| `" "` (clears line 1) | `msg.rs:348` | port literal = original's clear (op 3 line 0 / op 16 new box) | 10 §5.1 ops 3/16 | — (not text) |
| boss name string of op 8 | **not drawn** | msg op 8 `str`, ignored | 10 §5.1 op 8 | `"<t>_h1_<k>"` (dead in th08, §3.1) |
| boss name card | `face_stNN_name.png` | tex (stage face file script 1) | 10 §7 | `data/face/stNN/face_stNN_name.png` |
| song-title card | stgNtxt.anm sprite n+3 | tex | 10 §5.1 op 7, §8.4 | `data/text/stgNtxt.png` |

Speaker colours (table 0x4C7180) and box geometry stay the same. Line slot = op order (10 §5.1). The third line of a
box lands in the boss_card strip only in msg4ab entry 0, time 121 (10 §5.1 op 16 QUIRK).

### 2.2 Spell cards and bombs (07 §11, 12 §5.8, 14 §1.2)

| string | port site | source | spec | thcrap id |
|---|---|---|---|---|
| boss card name (story / spell practice) | `eclvm.rs:1508` → `gui.rs:236` → `gui.rs:276` (right-aligned) | ECL op 122 +0x14, 48 B XOR 0xAA | 07 §11, 12 §5.8 | `spells.js` `"<id>"`, id = op 122 +0x0E, 0-based (222/222) |
| 「リザレクション」 (name-only declaration) | `eclvm.rs:1304` → `gui.rs:236` | exe 0x4B495C | 07 §11.1, §12 (native 22) | `th08 Spell Resurrection` |
| player bomb names ×16 | `bomb.rs:187–216` → `gui.rs:199` | exe 0x4B43A0–0x4B4590 (`Bomb_Start` arg) | 14 §1.2, 14 §2.x, 12 §5.8 | `th08 Bomb Reimu`, `… Reimu Last`, `th08 Bomb Alice/Last`, `th06 Bomb Marisa B`, `th08 Bomb Marisa Last`, `th08 Bomb Remilia/Last`, `th07 Bomb Sakuya A focused`, `th08 Bomb Sakuya Last`, `th08 Bomb Yukari/Last`, `th08 Bomb Youmu/Last`, `th08 Bomb Yuyuko/Last` |
| 「ディゾルブスペル」 | `bomb.rs:220` → `gui.rs:199` | exe 0x4B43D4 | 14 §2.17 | `th08 Spell Dissolve` |
| "Bonus … history" label, name plates | ascii.anm sprites 133–135 | tex (`ascii.png`) | 12 §5.8 | `data/ascii/ascii.png` |
| bonus line digits | ascii.anm 0x88+d | tex | 12 §5.8 step 4 | — (digits) |

Card owner and comments are not shown in-game; they are stored (15 §2.x CATK) and shown in spell practice (§2.6).

### 2.3 Endings and staff roll (16 §1)

| string | port site | source | spec | thcrap id |
|---|---|---|---|---|
| ending text line | `ending.rs:386` → `ending.rs:436` | `.end` text line (Shift-JIS, `\0\n`) of `end0Nx.end` | 16 §1.3 | `end0Nx.end.jdiff` `"<file line>": {lines}` (§3.2) |
| "… ＥＮＤＩＮＧ　Ｎｏ．N" last line | same | `.end` line | 16 §1.7 | same file key |
| staff credits | staff01.anm sprites (`@a`) | tex `staff00a00..02.png` | 16 §1.7 | `data/end/staff00a0{0,1,2}.png` |
| ending pictures | `@b end0N*.jpg`, `staff00*.jpg` | tex (jpg) | 16 §1.3, §1.5 | RU only: `end03b1.jpg` |

### 2.4 Music room (16 §5)

| string | port site | source | spec | thcrap id |
|---|---|---|---|---|
| title list row (heard) | `musicroom.rs:51` → `:333` | musiccmt.txt entry title (+0x40) | 16 §5.1, §5.2 | `themes.js` `th08_%02d` = **entry + 1** (breakpoint `music_title`) |
| `？？？？？？？？` (unheard row) | `musicroom.rs:25,53` | exe 0x4B6EA0 | 16 §5.2 | `th08_????????` |
| comment line k (heard/playing) | `musicroom.rs:61` → `:235` | musiccmt comment k (+0x82 + k·0x42), cut to 64 bytes | 16 §5.1, §5.3 step 3 | `musiccmt.js` `"<entry+1>"`, `lines[k]` (`music_cmt`) |
| warning lines 1–7 (unheard) | `musicroom.rs:28–36` → `:235` | exe table 0x4C7AAC → 0x4B6E68 … 0x4B6D58 | 16 §5.3 step 3 | `th08 Music Room spoiler 1`…`7` (`spoiler 5` = `"　"`) |
| "now playing" title | `musicroom.rs:276` → `:282` | musiccmt comment line 0 (`No.N <title>`) | 16 §5.3 step 4 | `musiccmt.js` `lines[0]` = `"@"` → `Music Room Numbered Title` `"No. <r$%2d$99>  %s"` + themes (`music_title#nowplaying`) |
| `%2d.` row numbers, ▶ cursor | `musicroom.rs:400` (ascii) | exe 0x4B6E8C, byte 0x7F | 16 §5.2 | — |
| `Now Playing` | `musicroom.rs:407` (ascii) | exe 0x4B6E80 | 16 §5.2 step 4 | `th08 Now Playing` (no EN text) |
| background, labels | `music.jpg`, music00.png | tex | 16 §5.1 | `data/result/music00.png` |

### 2.5 Main menu descriptions (12 §2.2, §2.7, 15 §1.3)

All are exe literals, rendered centred (`menu.rs:455`). Port tables: `menu.rs:71` `DESCRIPTIONS` (top menu, 9),
`menu.rs:84` `OPTION_DESCRIPTIONS` (10), `menu.rs:98` `KEY_DESCRIPTIONS` (12).

| port table index → string | exe VA | thcrap id |
|---|---|---|
| top 0 ゲームを開始します | 0x4B8F1C | `th07 Menu Start` |
| top 1 エキストラステージを開始します | 0x4B8EFC | `th07 Menu Extra Start` |
| top 2 敵にスペルカード戦をお願いします | 0x4B8ED8 | `th08 Menu Spell Practice` |
| top 3 ステージを選択し、練習を開始します | 0x4B8EB4 | `th07 Menu Practice Start` |
| top 4 リプレイを鑑賞できます | 0x4B8E9C | `th07 Menu Replay` |
| top 5 過去のスコアや…取得歴を見られます | 0x4B8E6C | `th07 Menu Result` |
| top 6 音楽を聴けます | 0x4B8E5C | `th07 Menu Music Room` |
| top 7 各種設定できます | 0x4B8E48 | `th07 Menu Option` |
| top 8 いろいろと終了します | 0x4B8E30 | `th07 Menu Quit` |
| option 0 プレイヤーの初期数… | 0x4B90D0 | `th07 Option Player` |
| option 1 画面の色数… | 0x4B9090 | `th07 Option Graphic` |
| option 2 ＢＧＭの再生方法… | 0x4B905C | `th07 Option BGM` |
| option 3 ＢＧＭの音量… | 0x4B9040 | `th08 Option BGM Volume` |
| option 4 効果音の音量… | 0x4B9024 | `th08 Option SE Volume` |
| option 5 ウィンドウかフルスクリーンか… | 0x4B8FFC | `th07 Option Window Mode` |
| option 6 弾が多い場面で… | 0x4B8FBC | `th07 Option Slow Mode` |
| option 7 全て初期設定にします | 0x4B8FA4 | `th07 Option Reset` |
| option 8 パッド操作の… | 0x4B8F80 | `th07 Option Key Config` |
| option 9 おいそれと終了します | 0x4B8F68 | `th07 Option Quit` |
| key 0 ショット、決定… | 0x4B926C | `th07 Key Shot` |
| key 1 ボム、キャンセル… | 0x4B9248 | `th07 Key Bomb` |
| key 2 低速移動… | 0x4B922C | `th07 Key Slow` |
| key 3 メッセージスキップ… | 0x4B9204 | `th07 Key Skip` |
| key 4 ポーズ… | 0x4B91E8 | `th07 Key Pause` |
| key 5–8 上/下/左/右移動… | 0x4B91CC / 0x4B91B0 / 0x4B9194 / 0x4B9178 | `th07 Key Up/Down/Left/Right` |
| key 9 ショット押しっぱなしで… | 0x4B9144 | `th07 Key ShotSlow` |
| key 10 初期設定に戻します | 0x4B9130 | `th07 Key Reset` |
| key 11 おおよそ終了します | 0x4B911C | `th07 Key Quit` |

Menu item labels, the difficulty / character select and headings are textures: title01.anm entries `title02.png`
(#0), `title01.png` (#1, RU), `replay00.png` (#2), `sl_pltxt0..11.png` (#11–22, character descriptions, 12 §2.1
VMs 119–130), `select01.png` (#23, difficulty plates, 12 §2.3), `sl_text.png` (#24, headings, 12 §2.1 VMs 136–141).
Also `title00.png` and `th08logo.jpg` (03 §1, 04). thcrap: `data/title/*.png` (`title02.v1.00d.png` → chunk
`title02.png`).

### 2.6 Spell practice, screens 16/17 (16 §6, 12 §2.6)

| string | port site | source | spec | thcrap id |
|---|---|---|---|---|
| ステージ    取得/挑戦可能/総数（()内は全キャラ合計） | `menulist.rs:702` → `:517` | exe 0x4B92C0 | 16 §6.4 | `th08 Spell Practice Description` |
| 取得率　（キャラ別／総合） | `menulist.rs:704` | exe 0x4B92A4 | 16 §6.4 | `th08 Spell Practice Percentages` |
| row names `Stage1   … Last Word` (ascii) | `menulist.rs:37,820` | exe table 0x4C8274 | 16 §6.4 | — |
| `%sTotal` (ascii) | `menulist.rs:843` | exe 0x4B95D8 | 16 §6.4 | `th08 Spell Practice Total` |
| card row name | `menulist.rs:937` → `:959` | **score.dat CATK +0x10** (if attempted) | 16 §6.5, 15 §2.x | `spells.js` by card id (`spell_name#practice_menu_page`) |
| 挑戦者歓迎中！ (unlocked Last Word, never seen) | `menulist.rs:942` | exe 0x4B9320 | 16 §6.5 | — (no stringlocs id) |
| `？？？？？？？？` | `menulist.rs:303` | exe 0x4B6EA0 | 16 §6.3/§6.5 | `th08_????????` |
| `？？？？？？？？？` | `menulist.rs:304` | exe 0x4B9330 | 16 §6.3 | `th08_?????????` |
| `%sNo.%.3d` card number (ascii) | `menulist.rs:1216` | exe 0x4B962C | 16 §6.5 | `th08 Spell Practice Card No` |
| 取得数/挑戦数（()内は本編での集計） | `menulist.rs:1089` | exe 0x4B92FC | 16 §6.5 | `th08 Spell Practice List Description` |
| info 0 `Ｎｏ．%s　　%s` | `menulist.rs:993` → `:1052` | exe 0x4B9448 + full-width digits + CATK name | 16 §6.3 | `th08_spell_description_string_format` + `spells.js` (`spell_name#practice_menu_details`) |
| info 1 `使用者  %s　　%s %s` | `menulist.rs:998` | exe 0x4B942C + CATK +0x40 owner + `DIFF_NAMES` + `Last` | 16 §6.3 | `th08 Spell Practice Owner/Difficulty Line`; owner → `spellcomments.js` `owner` (`spell_owner`); `th08 Spell Practice Last Spell` |
| difficulty names 初月（イージー）… 極意（ラストワード） | `menulist.rs:77` | exe table 0x4C8628 → 0x4B86DC … 0x4B8670 | 16 §6.3 | `th08 Spell Practice Easy/Normal/Hard/Lunatic/Extra Mode`, `… Last Word` |
| info 2 `取得数/挑戦数[最高点]　%s　全主人公合計` | `menulist.rs:1007` | exe 0x4B9404 + `SHOT_NAMES` | 16 §6.3 | `th08 Spell Practice Highscore Title` |
| shot names (12 + total) | `menulist.rs:61` | exe table 0x4C7F4C → 0x4B732C … 0x4B723C | 16 §6.3, §2.2 | `th08 Stats Reimu & Yukari` … `th08 Stats Total (All Characters)` |
| info 3 counters line | `menulist.rs:1010–1035` | port format of exe formats (digits, `---`) | 16 §6.3 | — (numbers) |
| info 4 カードの補足 | `menulist.rs:1037` | exe 0x4B9344 | 16 §6.3 | `th08 Spell Practice Comment` |
| info 5/6 card comment | `menulist.rs:1043` | **score.dat CATK +0x70/+0xB0** | 16 §6.3, 15 §2.x | `spellcomments.js` `comment_1[0/1]` by card id (`spell_comment_line#1/#2`) |
| info 5/6 unlock hint (ids 204–221) | `menulist.rs:108` `HINTS` | exe table 0x4C82C8 (format ptr + 5 ints, printed +1) | 16 §6.3 | `th08 Spell Condition Line {1,2} <id+1>` (§3.5) |
| pie-chart frame, `Last`/difficulty images | title01.anm `select01.png` | tex | 16 §6.4 | `data/title/select01.png` |
| practice stage select `Stage    HI-Score` + rows (ascii) | `menulist.rs:619, 636` | exe 0x4B9598, stage table 0x4C8250 | 12 §2.5 | `th07 Practice Stage Header` |

### 2.7 Replay menu (12 §2.8, 15 §3)

| string | port site | source | spec | thcrap id |
|---|---|---|---|---|
| `No.   Name       Date  Player   Rank` (ascii) | `menulist.rs:1474` | exe 0x4B9560 | 12 §2.8 | `th07 Replay Header` |
| list row / `User ` / `Spell No.%3d` (ascii) | `menulist.rs:1488–1520` | replay header fields + exe formats, `CHARS` 0x4C86C0, `DIFFS` 0x4C86F0 | 12 §2.8 | `th08 Spell Replay` (`%s %8s  %6s %7s  Spell No.%3d`) |
| `Stage    LastScore` + rows (ascii) | `menulist.rs:1528–1554` | exe 0x4B9504, `STAGES` 0x4C8250, `No.%.3d %9d0` 0x4B94DC | 12 §2.8 | `th07 Replay Stage Header`, `th08 Replay Spell Number` |
| spell replay card name | `menulist.rs:1371` → `:1372` | **replay header +0x7E** (48 B = CATK name at save time) | 12 §2.8 step 3, 15 §3.6 | `spells.js` by header +0x7C (`spell_name#replay`) |
| replay player name | (ascii) | typed (replay header +0x72) | 15 §3 | — (never translated) |

### 2.8 Result / score screens (16 §2)

| string | port site | source | spec | thcrap id |
|---|---|---|---|---|
| card list row names | `result.rs:579` → `:584` | **score.dat CATK +0x10** | 16 §2.4 | `spells.js` (`spell_name#result`) |
| `？？？？？` | `result.rs:77,582` | exe 0x4B77D8 | 16 §2.4 | — (no stringlocs id) |
| `%3d/%3d（キャラ切り替え↓↑）` | `result.rs:586` → `:590` | exe 0x4B795C | 16 §2.4 | `th08 Spell Result Character Select` |
| `No.%.2d`, `(%s)`, `MaxBonus %8d` (ascii) | `result.rs:1260, 1264` | exe 0x4B7C78, 0x4B7C44 | 16 §2.4 | `th06_ascii_2_digit_number_format`, `th07 Max Bonus` |
| play data line 0 総起動時間 | `result.rs:1571` → `:644/:666` | exe 0x4B7A9C | 16 §2.4 | `th08 Stats Time Since Startup` |
| line 1 総プレイ時間 | `result.rs:1575` | exe 0x4B7A80 | 16 §2.4 | `th08 Stats Total Playtime` |
| line 2 プレイ回数 header | `result.rs:1579` | exe 0x4B7A44 | 16 §2.4 | `th08 Stats Play Count` |
| lines 3–15 shot names + `%s %6d…` | `result.rs:1580–1581` | exe 0x4B7A28 + `SHOT_NAMES` | 16 §2.4 | `th08 Stats Character Format` + `th08 Stats <shot>` |
| line 16 クリア回数 | `result.rs:1582` | exe 0x4B79FC | 16 §2.4 | `th08 Stats Clear Count` |
| line 17 コンティニュー | `result.rs:1583` | exe 0x4B79D0 | 16 §2.4 | `th08 Stats Continue` |
| line 18 プラクティス | `result.rs:1584` | exe 0x4B79A4 | 16 §2.4 | `th08 Stats Practice` |
| ranking header + rows (ascii) | `result.rs:1046` | exe 0x4B7CD4, typed names, `%m/%d` 0x4B797C | 16 §2.3 | `th08 Results Header`, `th07 Date Format` |
| replay-save header / rows (ascii) | `result.rs:1137–1171` | exe 0x4B7C1C, 0x4B7C00, 0x4B7BC8 | 16 §2.6 | `th07 Replay Save Header`, `th07 Replay Save`, `th07 Replay Save Empty` |
| end panel difficulty `      Easy` … (ascii) | `result.rs:68` | exe 0x4B7AE8 table 0x4C7FFC | 16 §2.5 | — |
| end-panel labels, result headings | result00.anm `result00/02/03.png` | tex (labels are in result03.png, 16 §2.5) | 16 §2.3–§2.5 | `data/result/result0{0,2,3}.png` |
| name-entry charset (ascii) | `result.rs:63` | exe 0x4B7340 | 16 §2.2 | — (input glyphs; never translated) |

### 2.9 HUD and in-game ASCII strings (12 §5)

All are drawn with ascii.anm glyphs (§1.2). They are already English in the original. thcrap's stringdefs have
**no EN text** for them, so they count as Latin-only.

| string | port site | exe VA | spec | thcrap id |
|---|---|---|---|---|
| `Stage Clear` / `All Clear!` | `hud.rs:960, 962` | 0x4B4F1C / 0x4B4F10 | 12 §5.6 | `th07 Stage Clear` / `th07 All Clear` |
| `Clear = %8d0`, `Point = %8d0`, `Graze = %8d0`, `Time  = %8d0` | `hud.rs:970–989` | 0x4B4F00, 0x4B4EF0, 0x4B4EE0, 0x4B4ED0 | 12 §5.6 | `th07 Clear Bonus/Point Items/Graze Format`, `th08 Clear Time Orbs Format` |
| `over-80%% = …` / `over 80%% = …` | `hud.rs:1001, 1004` | 0x4B4EB8 / 0x4B4EA0 | 12 §5.6 | `th08 Clear Human/Youkai Format` |
| `Player =%8d0`, `Bomb   = %7d0` | `hud.rs:1011, 1013` | 0x4B4E90, 0x4B4E80 | 12 §5.6 | `th07 Clear Bonus Life/Bomb Format` |
| `Last Time = %2d:%.2d`, `Night Bonus` | `hud.rs:1020, 1024` | 0x4B4E68, 0x4B4E5C | 12 §5.6 | `th08 Clear Last Time Format`, `th08 Clear Night Bonus` |
| `Easy Rank    *0.5` … `Phantasm Rank*2.0` | `hud.rs:32–37` | 0x4B4E38 … 0x4B4DD4 | 12 §5.6 | `th07 Clear <diff> Multiplier` |
| `Player Penalty*0.5/0.2/0.1/0.05` | `hud.rs:1036–1039` | 0x4B4DC0 … 0x4B4D84 | 12 §5.6 | `th07 Clear Player Penalty*…` |
| `Total = %8d0` | `hud.rs:1048` | 0x4B4D74 | 12 §5.6 | `th07 Clear Total Score Format` |
| night-time change `%s >> %s` | `hud.rs:1060–1062` | — | 12 §5.6 | — |
| ` BONUS %8d` popup | `hud.rs:908` | 0x4B4FC8 | 12 §5.5 | — |
| `Full Power Mode!` | `hud.rs:914` | 0x4B4FB4 | 12 §5.5 | `th07 Full Power` |
| `Supernatural Border!!`, `CherryPoint Max!`, `Border Bonus %7d` (types 2–4, never shown) | `hud.rs:916–918` | 0x4B4F98, 0x4B4F84, 0x4B4F70 | 12 §5.5 | — |
| `Spell Bonus Failed` / `Last Spell Failed` | `hud.rs:919, 920` | 0x4B4F5C / 0x4B4F48 | 12 §5.5, 07 §11.1 | `th08 Spell Bonus Failed` / `th08 Last Spell Failed` |
| `Spell Card Bonus!` + `+%d` | `hud.rs:934–943` | 0x4B4F30 | 12 §5.5 | `th06_ascii_centered_spell_bonus` |
| panel numbers, `MAX` | `hud.rs:1076–1127` | 0x4B4CD8 (`MAX`) | 12 §5.3 | `th07 MAX` |
| boss timer digits | `hud.rs:831` | — | 12 §5.4 | — |
| `Press Shot Button` (loading) | `ascii.rs:208` | 0x4B6628 | 12 §1 | — |
| replay fps `%2d` | `supervisor.rs:294` | — | 15 §3.3 | — |
| gauge `%`, `ENEMY` markers | `ascii.rs:897–960`, 12 §5.10 sprites | tex | 12 §5.9, §5.10 | — |
| HUD side panel labels, difficulty, clock | front.anm (`front.png`, `ename.png`), times.anm, ascii.anm 0x19/0x1A | tex | 12 §5.2, §5.7 | `data/front/front.png`, `data/front/ename.png` (Q1), `data/text/times.png` |
| stage title / song cards | stgNtxt.anm | tex | 12 §5.1a, 10 §5.1 op 7 | `data/text/stg{1..8}txt.png` (4a/4b: `stg4atxt/stg4btxt`) |
| loading screen | nowloading.anm `loading.png` | tex | 12 §1 | `data/ascii/loading.png` |

### 2.10 Pause and continue menus (12 §3, §4)

These are textures only: ascii.anm scripts 12–24 with sprites of `pause.png` (ascii.anm entry #2): title, resume /
quit / retry, yes / no, "continue?", practice / slow labels (sprites 289/290), difficulty 0x11B+d. thcrap:
`data/ascii/pause.png`. No GDI text and no ascii strings.

### 2.11 Texture chunk names (keys for image overrides)

Chunk name = the ANM entry name (03 §2.1). thcrap's paths match it 1:1 (`re/extracted/anm_index.txt`):

| thcrap image | ANM file #entry | shows (spec) |
|---|---|---|
| `ascii/ascii.png` (EN + `script_latin`), RU also `asciis.png` | ascii.anm #0 / #1 | ASCII glyphs, plates, labels (12 §2.9, §5.8) |
| `ascii/pause.png` | ascii.anm #2 | pause/continue menu (12 §3–§4) |
| `ascii/loading.png` | nowloading.anm #0 | loading indicator (12 §1, 04) |
| `end/staff00a0{0,1,2}.png` | staff01.anm #0–2 | staff roll credits (16 §1.7) |
| `face/stNN/face_stNN_name.png` | face_stNN.anm #1 | boss name card (10 §7) |
| `front/front.png`, `front/ename.png` | front.anm #0 / #1 | HUD (12 §5.2) |
| `result/music00.png` | music00.anm #0 | music room (16 §5) |
| `result/result00/02/03.png` | result00.anm #0/#2/#3 | result screens (16 §2) |
| `text/stgNtxt.png`, `text/times.png` | stgNtxt.anm #0, times.anm #0 | stage titles, night clock (12 §5) |
| `title/replay00, select01, sl_pltxt0..11, sl_text, title02(.v1.00d), title01 (RU)` | title01.anm #2, #23, #11–22, #24, #0, #1 | main menu (12 §2.1) |
| RU `etama/etama3.png` | etama.anm #2 | bullet sheet (contains text? Q2) |
| RU `face/face_cdbg.png` | face_cdbg.anm #0 | cut-in streak strips (12 §5.8) |
| `info0{0,1,2}.jpg`, RU `title00.png`, `th08logo.jpg`, `end03b1.jpg` | standalone files | unlock notices (16 §4), title (03 §1), ending picture (16 §1.3) |

### 2.12 Strings that are not displayed or go to files (must stay vanilla)

| string | site | why it stays Japanese/raw |
|---|---|---|
| replay USER section `キャラ名`, `難易度`, `最終ステージ`, `カード名 No.%3d <name>`, `プレイヤー名`, … | `replay.rs:300–347` | file content (15 §3.5). A replay must be byte-identical in every language |
| replay header +0x7E card name | `replay.rs:287` | copied from CATK (15 §3.6) |
| score.dat CATK name/owner/comments | `scoredat.rs`, `eclvm.rs` Attempt | persistent and checksummed (§4.1) |
| score.txt dump | `result.rs:1589–1780` | file output (16 §2.7) |
| log/error strings (`リプレイデータが異常`, `error : データのバージョンが違います`, …) | `menulist.rs:1363`, `game.rs:288` | log only |
| file names (`replay/th8_%.2d.rpy`, `score.dat`, `th8_ud????.rpy`) | — | thcrap ids `th08_replay_*`, `th08_scorefile_fn` must never be applied |

## 3. thcrap key mapping (evidence for G3)

### 3.1 msg jdiff keys

Model (scratch script, run against `re/extracted/dat/msg*.dat` and all 32 `<thcrap>/thpatch/lang_en/th08/msg*.jdiff`):
per entry, walk the ops in file order.
- ops 4 and 15 (`auto_end`) close the current box;
- ops 16/19/20 (`auto_line`) open a new box when none is open **or the op's time differs from the box's start
  time**, else they append a line;
- op 3 (`hard_line`) opens a new box when its `line` arg is 0, else it appends to the current box, regardless of time
  or closing ops (msg1a entry 0: box `63_1` = op 3 line 0 at t 63 + op 3 line 1 at t 124);
- box key = `<start time>_<k>`, k = number of boxes already started at that time in this entry;
- op 8 → `<time>_h1_<k>` (`msg06` format in `<thcrap>/nmlgc/base_tsa/formats.js`: `"8": {"cmd": "hard_line", "type": "h1"}`).

Result: 1535 / 1535 jdiff keys reproduced, 0 missing. 78 boxes have more translated lines than ops (thcrap inserts
ops; our converter joins them into the last slot, I18N §2.3). 408 vanilla boxes have no key: entries that share an
offset, plus the `h1` keys of b/c/d files. msg files list entry offsets that can repeat (10 §2): e.g. msg6a
`(28, 28, …)`, msg7a `(44, 44, …)`. thcrap translates only the lowest index (msg6a jdiff has entries 0, 2, 3 and no 1),
so entry 1 must reuse entry 0's text (same bytes).

**`h1` = op 8 `boss_card`.** EN keys `124_h1_0` msg1a (`　闇に蠢く光の虫　` → "Insect of Light Wriggling in Darkness"),
`61_h1_0` msg2a (Mystia), `64_h1_0` msg3a (Keine), `122_h1_0` msg4ba, `123_h1_0` msg5a/6a, `124_h1_0` msg7a.
For th08 the `msg08` format (`formats.js:14`) lists only ops 3, 4, 15, 16, 19, 20, so thcrap never matches op 8. The
game ignores the string anyway (10 §5.1 op 8: "The arguments are ignored"). The boss-name card is the texture
(10 §7). In 10 §5.1 it maps to op 8 and takes **no text slot**. Converter: skip `_h1_` keys and count them in `pack-check`.

Slot for TextRef `Msg{file, entry, nth}`: nth = index among the entry's text ops (3, 16, 19, 20) in file order. The
on-screen line (0/1/boss_card strip) is runtime state (op 16 `next_line`, which SKIP changes: op 4's skip path does not
set new_box, 10 §5.2). Keying by op index keeps a translation stable regardless of that.

### 3.2 end jdiff keys

Key = 0-based line number of the `.end` file split on LF, pointing at the **first line of a block of consecutive text
lines** (non-`@`, non-empty). `lines` fills that block's lines in order. Checked: 367 keys = 367 blocks, all at a block
start. 32 blocks have more translated lines than vanilla, 5 have fewer. Text markup `<l$Name:$12345678>` and leading
U+3000 indents occur (I18N §5). TextRef `Ending{file, line}` should use the same physical line number per text line.

### 3.3 spells.js / spellcomments.js

`spells.js` `"0".."221"` = ECL op 122 +0x0E card id, 0-based (`"0"` = 蛍符「地上の流星」, ecl_dis.txt:484). 222/222.
`spellcomments.js` id → `{comment_1: [2 lines], owner}`. In-game, these replace the card plate (ECL bytes); in
spell practice, result and replay they replace the **CATK / replay header bytes**. All of these are looked up by
**card id**, never by the stored bytes (§4.1).

### 3.4 musiccmt.js / themes.js

`musiccmt.js` `"1".."21"` = music-room entry index + 1. `lines[k]` = comment k. `lines[0] = "@"` keeps line 0 = the
`No.N title` line that thcrap generates from `themes.js` + `Music Room Numbered Title`. `themes.js` `th08_NN`, NN =
**entry + 1**, not the bgm file name: `th08_02` = entry 1 = `th08_00.mid` 幻視の夜, `th08_16` = entry 15 =
`th08_13b.mid` Voyage 1970, `th08_19` = entry 18 = `th08_16.mid` 月見草 (16 §5.1 table). The title row (+0x40) maps to
themes; comment line 0 maps to the numbered title.

### 3.5 stringlocs ↔ port Builtin

`Rx<rva>` → VA = 0x400000 + rva (checked: `Rxb6e80` = 0x4B6E80 "Now Playing"). The th08 ids map to the port tables in
§2.4–§2.9. Traps:
- `th08 Spell Condition Line {1,2} NNN` ids are **1-based card numbers** (NNN = card id + 1, as printed on screen;
  16 §6.3). Line 2 ids are shared across cards (`… Line 2 206 207 209`). Several are printf formats (`No.%.3d`); the port
  `HINTS` has the numbers already printed, so `Builtin{id, args}` needs the 5 ints of table 0x4C82C8.
- `th08 Stats Play Count`, `… Time Since Startup`, `… Total Playtime` EN values drop or reorder the column layout and
  add `<l$…>`/`<tl$…>` markup. The column alignment of the vanilla line (spaces in the format, 10 §6) does not survive.
- `th08 Stats Character Format` and the ascii formats have no EN text (Latin already).
- Strings with no stringlocs id that are still displayed: 挑戦者歓迎中！ (0x4B9320), `？？？？？` (0x4B77D8), ascii
  headers `Stage1…`, `Rm & Yk…`, difficulty columns, `Press Shot Button`. A port `Builtin` table needs its own ids for
  them.

## 4. Does text feed logic or RNG? (deliverable 2)

### 4.1 Cases where string content reaches logic or persistent state, all from logic-key bytes

| # | case | spec | source | translation impact |
|---|---|---|---|---|
| L1 | CATK checksum byte = (Σ signed **card-name bytes** up to NUL + Σ 78 counters) & 0xFF. A mismatch at `Spell_Start` **zeroes all 78 counters** | 15 §2 CATK table, `Spell_Start` order steps 1–4 | ECL op 122 name (strcpy into CATK +0x10) | none while names come from ECL. **A port must never write a translated name into CATK.** Foreign score.dat: Q4 |
| L2 | CATK owner/comments stored (+0x40, +0x70/+0xB0) | 15 §2 steps 2–3 | ECL op 122 | as L1 (stored bytes only) |
| L3 | replay header +0x7E card name = CATK +0x10 bytes; replay USER section strings | 15 §3.5, §3.6 | CATK, exe | file bytes; display maps by card id (+0x7C) |
| L4 | ending: a text line with ≥ 1 copied byte pair → render + **interrupt 1 on VM[line]**; empty line → no render, still costs a VM index and a line wait | 16 §1.3 | `.end` bytes | decided on raw bytes; a pack must not change it (I18N §2.1: line slots fixed) |
| L5 | music room: comment first byte 0 → VM flag bit 1 cleared, else set | 16 §5.3 step 3 | musiccmt bytes / exe warning | raw bytes decide. A translated empty line must not hide a non-empty raw line and vice versa (draw only) |
| L6 | msg text ops reset `wait_count` (box timing), op 16 uses `next_line` | 10 §5.1, §5.2 | op sequence, not content | none (no ops added; I18N §2.1) |
| L7 | player name entry → score.dat / replay | 16 §2.2, 15 | typed (CHARSET 0x4B7340) | never translated |
| L8 | ECL name length is logged (`eclvm.rs:1520`, port log only) | — | ECL | none |

### 4.2 Widths and positions, all draw-only

| quantity | spec | used by |
|---|---|---|
| boss card name x = trunc(320 − len·15/2) | 10 §6, 12 §5.8 | `TextCmd.x` only |
| `Gui` +0x2618 / +0x261C := len·15/2 + 16 | 12 §5.8 "(never read)" | not modelled in the port |
| description x = trunc(256 − len·15/4) | 10 §6, 12 §2.2 | `TextCmd.x` only |
| GDI DIB / D3DX copy, W = 512, whole strip replaced | 10 §6 | render (texture contents never read back by logic; the capture of 12 §3 copies the back buffer into a texture, draw state) |
| ascii advance 13 px (+0x8280, read at draw time), '\n' 16 px, 64-byte copy, 256-entry queue | 12 §2.9 | `emit_texts`; thcrap `ascii_params` `CharWidth` is this advance → per-pack presentation constant |
| ascii popup x from glyph counts (`ascii.rs:384, 506, 521`), `Spell Card Bonus!` x = (384 − 17·14)/2 + 32 | 12 §5.5 | draw positions of fixed-content strings |
| panel `/` x = 488 + 13·len (`hud.rs:1509`) | 12 §5.3 | draw only (digits) |
| `ascii::text_width` (`ascii.rs:964`) | 12 §2.9 | no callers |
| msg ops 1/2 `pos_offset` from **sprite** width > 128/256 | 10 §5.1 note | ANM sprite table (logic ANM), not the image; no visible effect. Image overrides keep the logical size (ARCH_B §1.7) |
| comment cut to 64 bytes, vsprintf buffers 128/72 bytes, `.end` 0x44 buffer | 16 §5.1, 10 §6, 16 §1.3 | vanilla-path limits; translated text bypasses them (rendered from the pack, never formatted) |

No text width, glyph count, ascii position or string content is an input to the RNG. RNG consumers are listed in
11 §1.5. The only text-adjacent consumers are ANM VMs running scripts (ops 59/60). Text VMs start their scripts per
**op / line slot / cursor**, never per character or per pixel (10 §5.1, 12 §5.8, 16 §1.3, §5.3, §6.3). The
`Dialogue.texts` count in `worldclone.rs:202` (determinism test) counts ops, not content.

### 4.3 Consequence for G1/G2

The world holds raw text in three queues: `Dialogue.texts` (`msg.rs:158`), `Gui.texts` (`gui.rs:61`, with the
precomputed vanilla x) and the ascii queue (`ctx.w.ascii`). They must keep the raw logic bytes. `TextRef` travels next
to the raw bytes. The renderer swaps in the text and recomputes x (right-align for the boss card, centring for
descriptions) from the measured width. Score.dat, replay and score.txt writers keep reading raw bytes.
