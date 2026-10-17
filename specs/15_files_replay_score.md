# 15 — Files: th08.cfg, score.dat, replays

Tools in `tools/`: `cfg.py` (th08.cfg check), `scoredat.py` (score.dat decode and validation), `rpy.py` (replay decode and validation).

Current results:
- the real `th08.cfg`: 0 problems
- the real `score.dat`: 0 problems
- `demorpy0..3.rpy`: 0 problems each

Addresses are VAs in th08.exe. GM = game manager 0x160F508.

---

## 1. th08.cfg — VERIFIED (`Config_Load` 0x4477B1)

### 1.1 Layout

The file is 60 bytes (0x3C). In memory it lives at 0x17CE870 (= Supervisor+0x118). `*(GM+4)` points to the copy used by gameplay.

| off | type | field | valid | default |
|---|---|---|---|---|
| 0x00 | i16 | pad button → Shot | — | 0 |
| 0x02 | i16 | pad button → Bomb | — | 1 |
| 0x04 | i16 | pad button → Focus | — | 2 |
| 0x06 | i16 | pad button → Pause (ESC) | — | 4 |
| 0x08 | i16 | pad button → Up | — | −1 |
| 0x0A | i16 | pad button → Down | — | −1 |
| 0x0C | i16 | pad button → Left | — | −1 |
| 0x0E | i16 | pad button → Right | — | −1 |
| 0x10 | i16 | pad button → Skip (Ctrl) | — | 3 |
| 0x12 | 2 B | padding | | |
| 0x14 | u32 | version | must be 0x00080001 | 0x80001 |
| 0x18 | i16 | analog stick X threshold | — | 600 |
| 0x1A | i16 | analog stick Y threshold | — | 600 |
| 0x1C | u8 | starting lives option | < 7 | 2 |
| 0x1D | u8 | unused (reset to 3 by the option "Reset") | < 4 | 3 |
| 0x1E | u8 | force 16-bit colour | < 2 | 0; forced to 1 if the desktop is 16-bit |
| 0x1F | u8 | music: 0 off, 1 WAV (thbgm.dat), 2 MIDI | < 3 | 1 if `thbgm.dat` has a valid header ("ZWAV", version 1, 0x800), else 2 |
| 0x20 | u8 | sound effects on | < 2 | 1 |
| 0x21 | u8 | last chosen difficulty | < 6 | 1 |
| 0x22 | u8 | windowed | < 2 | 0 |
| 0x23 | u8 | frame skip: the draw chain runs when an i8 counter (incremented every frame) reaches this value, then the counter is reset. So 0 **and** 1 = draw every frame, 2 = every 2nd frame (VERIFIED `Frame_Run`, answers_phase7 §6). Not in the option menu. | < 3 | 0 |
| 0x24 | u8 | effect level (0 none, 1 half, 2 all; see 13 §2) | < 3 | 2 |
| 0x25 | u8 | slow mode | < 2 | 0 |
| 0x26 | u8 | joystick "shot also focuses" toggle; only read by `Input_PollJoystick` (answers_phase7 §5) — VERIFIED | < 2 | 0 at load, 1 after a key-config reset |
| 0x27 | u8 | BGM volume 0..100 | — | 100 |
| 0x28 | u8 | SE volume 0..100 | — | 80 |
| 0x29..0x37 | | unused | | 0 |
| 0x38 | u32 | debug/driver flags | — | bit 0 is always forced to 1 |

Pad buttons are DirectInput button indices; −1 means no button.

Debug flags at +0x38 (VERIFIED from the log strings printed at load):

| bit | effect |
|---|---|
| 1 | no vertex buffer |
| 2 | force 16-bit textures |
| 3, 4 | clear the back buffer |
| 5 | Gouraud shading off |
| 6 | depth test off |
| 8 | colour composite off |
| 9 | (logged) |
| 11–15 | DirectInput off, BGM read into memory, bit 14 = no vsync, … |

The real file has 0x4001.

### 1.2 Load and save

At boot the game reads `th08.cfg` from disk. The whole file is reset to defaults when any of these hold:
- the file is missing;
- the size is not 0x3C;
- the version is wrong;
- any byte field is out of its valid range.

When that happens it logs "コンフィグデータが異常". After the music-mode probe it writes the file back straight away. It writes the file again when WinMain exits.

### 1.3 Option menu (`MainMenu_Option` 0x46837B) — VERIFIED

| item | effect |
|---|---|
| 0 Player (lives) | Cycles +0x1C. The maximum depends on the total play count (PLST total block [0], 0x164CED0): < 30 → 0..4, < 60 → 0..5, otherwise 0..6. |
| 1 Color | Toggles +0x1E. |
| 2 BGM | Cycles +0x1F. Any change restarts `bgm/th08_01.mid` (title music). |
| 3 BGM volume, 4 SE volume | ±1 per press; key repeat starts after 30 frames held; the repeat path steps ±4. Range 0..100. Sends the set-volume command. |
| 5 Mode | Toggles window mode +0x22. |
| 6 Slow | Toggles +0x25. |
| 7 Reset | Sets lives 2, +0x1D 3, music 1, SE 1, slow 0. |
| 8 KeyConfig | Opens key config (0x469636): 12 rows = 9 buttons from §1.1, the +0x26 toggle, Default (reset to the table at 0x4C78C8 and set +0x26 = 1), Quit. |
| 9 Quit | Leaves the option menu. |

When leaving the menu, if +0x1E or +0x22 changed, the function returns 5. That makes the Supervisor recreate the D3D device. After 3600 idle frames the menu falls back to the top menu.

---

## 2. score.dat — VERIFIED

### 2.1 Container

Load: `ScoreDat_Load` 0x45A5E0. Save: `ScoreDat_Save` 0x453D0D (`this` = Result object).

The file is encrypted on disk with two layers, outermost first:
1. Archive block cipher `Crypt(key 0x59, step 0x79, block 0x100, limit 0xC00)` over the whole file (01 §3). Decrypt with 0x43E1D0, encrypt with 0x43E4A0.
2. A rolling XOR that starts at byte 1:
   - Decode: `acc = 0`; for i = 1 .. size−2: `acc = rotl8(acc + b[i], 3)`, then `b[i+1] ^= acc`. Because the loop runs forward, `b[i]` is already decoded (plain) when it is used, and b[1] is never changed.
   - Encode (save): the same running `acc`, computed from the plain bytes; each byte from index 2 on is XOR-ed with it. The saver processes the bytes so that the decoder above recovers them (VERIFIED by round-trip with the real file).

Header after decoding (0x1C bytes):

| off | type | value |
|---|---|---|
| 0x00 | u8 | 0 |
| 0x01 | u8 | random (Rng_Range16(256)); because of the XOR it acts as the key |
| 0x02 | u16 | checksum = sum of bytes 4..end of the decoded **compressed** file, mod 65536 |
| 0x04 | u16 | version 1 |
| 0x06 | u8 | random (Rng_Range16(256)) |
| 0x08 | u32 | header size 0x1C |
| 0x0C | u32 | runtime pointer, garbage |
| 0x10 | u32 | decompressed size including the header |
| 0x14 | u32 | decompressed body size (= +0x10 − 0x1C) |
| 0x18 | u32 | compressed size (= file size − 0x1C) |

The body from +0x1C is LZSS (01 §4). It decompresses to a list of chunks.

**Saving consumes 2 RNG draws** (two Rng_Range16(256)). Saves happen only outside gameplay, but the RNG state carries over into the next game (11 §1.5).

Each chunk starts with this header:

| off | type | field |
|---|---|---|
| 0 | char[4] | magic |
| 4 | u16 | size |
| 6 | u16 | size (again) |
| 8 | u8 | version |
| 9 | 3 B | padding |

Load rejects the file, and the game then starts from defaults, when any of these hold:
- size < 0x1C;
- bad checksum;
- header size ≠ 0x1C;
- version ≠ 1;
- a chunk with size 0;
- the VRSM exe check fails (§2.10);
- no TH8K chunk with version 1.

Each `ScoreDat_Get*` walks the chunk list and only accepts a chunk whose magic and version match.

Write order (VERIFIED from the saver):
1. TH8K
2. HSCR × n
3. CLRD × 13
4. CATK × (cards whose magic is set)
5. PSCR × (shots whose used flag +0x175 is set)
6. LSNM
7. FLSP
8. PLST (the run-time clock is updated just before)
9. VRSM

| chunk | size | ver | count |
|---|---|---|---|
| TH8K | 0x0C | 1 | 1 (header only) |
| HSCR | 0x168 | 4 | top 10 per (difficulty 0..4, shot 0..11), default entries skipped |
| CLRD | 0x24 | 4 | 13 |
| CATK | 0x22C | 3 | up to 222 |
| PSCR | 0x178 | 2 | up to 12 |
| LSNM | 0x18 | 1 | 1 |
| FLSP | 0x20 | 1 | 1 |
| PLST | 0x228 | 2 | 1 |
| VRSM | 0x1C | 1 | 1 |

### 2.2 When the file is loaded and saved

| caller | what is taken from the file |
|---|---|
| Supervisor_OnRegister 0x445FA3 (boot) | PLST only. The block is zeroed first. |
| MainMenu_OnRegister 0x470A6C | CLRD, PSCR, CATK, FLSP, PLST. Then sets GM flags bit 15/16/17 (16 §4). |
| GameMgr_LoadScoreData 0x43BBE1 (new game) | Hiscore for (shot, difficulty) → stats+0x14; CATK, CLRD, PSCR. In practice the hiscore is PSCR[shot].hiscore[stage][diff] and that play count is incremented. Then backs up CATK, clears the in-game HSCR record (§2.3) and copies the config into it. |
| Result_OnRegister 0x45964D | Builds the 5×12 ranking lists and LSNM. In viewer mode (Result mode 0) also loads CATK, CLRD, PSCR. |

`Result_Register(mode)` 0x4582A0 is the only path to a save. Its on-remove handler 0x459FD2 calls `ScoreDat_Save`. The call sites are:
- mode 2 (flush only: load, then immediately save) from:
  - the pause menu "quit" (0x4045AC);
  - GameMgr_LoadScoreData (0x43BBF3, at game start);
  - WinMain exit (0x441B95).
- mode 1 from Supervisor scene 6 (after a game).
- mode 0 from scene 5 (result viewer from the menu).

In mode 2 with slow mode on (or Supervisor+0x1A4 bit 3, which retail code only sets through the anti-tamper fill, answers_phase7 §2), CATK is first reloaded from the old file, which throws away card credit from this run. VERIFIED.

### 2.3 HSCR (0x168) — high-score entry

| off | type | field |
|---|---|---|
| 0x0C | u32 | score ÷ 10 |
| 0x10 | f32 | slowdown % (§2.11) |
| 0x14 | u8 | shot 0..11 |
| 0x15 | u8 | difficulty 0..4 |
| 0x16 | u8 | stage reached: g_StageIdx (0..8), or **99** if the game was cleared (flags bit 4) |
| 0x17 | char[9] | name (8 + NUL) |
| 0x20 | char[6] | date "%m/%d" |
| 0x26 | u8 | continues used |
| 0x28 | 60 B | th08.cfg copy at game start (+0x44 = lives option) |
| 0x64 | u32 | play time in frames (0x164D09C) |
| 0x68 | u32 | point items collected (incremented in Item_CollectPoint 0x440E40) |
| 0x6C | u32 | never written (0) |
| 0x70 | u32 | misses (GM+0x3DA9C++) |
| 0x74 | u32 | bombs used |
| 0x78 | u32 | Last Spells used |
| 0x7C | u32 | pauses (GM+0x3DAA8++) |
| 0x80 | u32 | time-orb total (GM+0x3DAAC += n) |
| 0x84 | u32 | human rate × 100 (see below) |
| 0x88 | u8[222] | per card: captured during this game |
| 0x166 | u8 | 1 = default (placeholder) entry |

The in-game record is at 0x164CF34 (= GM+0x3DA2C). It is zeroed at new-game start (GameMgr_LoadScoreData). Shot and difficulty are filled at that point; the date, name and slowdown at name entry.

Human rate: EnemyMgr_Calc counts only outside dialogue and while its timer is > 15. Each frame it increments `total` (0x164D0AC), and also `human` (0x164D0A8) when the player is in human mode (player+3 == 0). Then rate = human·10000 / total. VERIFIED.

**Default table** (Result_OnRegister). For every difficulty d, shot s and rank i = 0..9:
- score (÷10) = 100000 − 10000·i;
- name "--------", date "--/--";
- stage 0, continues 0, version 4;
- +0x166 = 1.

The file's HSCR entries are then inserted into these lists in order. `ScoreList_Insert` 0x45A500 inserts into a singly linked list sorted by descending score (order among equal scores INFERRED: new entry after) and returns its 0-based rank. Only the first 10 are kept or saved; default entries (+0x166 = 1) are never written to the file.

`ScoreDat_GetHiscore` 0x45A950 returns max(top entry of (shot, difficulty), 100000), in ÷10 units. That value is the HUD "HiScore" at game start.

### 2.4 CLRD (0x24 × 13) — clear flags

The in-memory copy is at GM+0x3C490 (0x164B998). Entries 0..11 are per shot; entry 12 is the union over all shots.

| off | type | field |
|---|---|---|
| 0x0C | u16[5] | no-continue clear mask per difficulty (E, N, H, L, Ex) |
| 0x16 | u16[5] | clear mask (continues allowed) |
| 0x20 | u8 | "bad ending seen, good not yet" flag (16 §1) |
| 0x21 | u8 | entry index |

Every mask defaults to 1 (stage 1 bit).

Bits:
- `1 << stage_idx` is set when a stage is cleared, by GameMgr_Calc at the stage-clear transition. The no-continue mask is set only when continues == 0. This also happens in practice, but not in replays.
- 0x4000: Final B ending (6B / stage 7) reached.
- 0x8000: Final A ending (6A) reached. Both bits are set by the ending code 16 §1.
- Extra clear: no-continue[4] |= 0x8000 and union any[4] |= 0x8000.

Unlock checks (VERIFIED):

| fn | true when |
|---|---|
| 0x43C15F(team t) | for t < 4: any no-continue[E..L] & 0x4000. Returns true for t ≥ 4. |
| 0x43C23B(team t) | for t < 4: any any-clear[E..L] & 0x8000. Returns true for t ≥ 4. |
| 0x43C1E9 | some team 0..3 passes 0x43C15F → **Extra unlocked** (GM flags bit 15) |
| 0x43C2C5 | some team passes 0x43C23B → flags bit 16 |
| 0x447D04 | all teams 0..3 pass 0x43C15F → flags bit 17 (solo characters unlocked) |

The menu shows `title/info00.jpg`, `info02.jpg` or `info01.jpg` once when the matching condition newly becomes true (16 §4).

### 2.5 CATK (0x22C × 222) — card history

The in-memory copy is GM+0x40 (0x160F548). There is a backup at 0x162D770 (0x1E228 bytes) taken at game start; the backup is copied back when slow mode is on (Result modes 0x11/0x16/0x12).

| off | type | field |
|---|---|---|
| 0x0C | u16 | card id |
| 0x0E | u8 | checksum = (Σ signed name bytes [48] + Σ all 78 u32 counters) & 0xFF |
| 0x0F | u8 | difficulty of the last attempt |
| 0x10 | char[48] | card name (decoded from ECL with XOR 0xAA) |
| 0x40 | char[48] | owner (XOR 0xBB) |
| 0x70 | char[64] | comment line 1 (XOR 0xDD); written only on a spell-practice capture |
| 0xB0 | char[64] | comment line 2 (XOR 0xEE) |
| 0xF0 | u32[13] | story: max bonus per shot (index 12 = all shots) |
| 0x124 | u32[13] | story: attempts |
| 0x158 | u32[13] | story: captures |
| 0x18C | u32[13] | spell practice: max bonus |
| 0x1C0 | u32[13] | spell practice: attempts |
| 0x1F4 | u32[13] | spell practice: captures |

Counters are capped at 9999.

- Attempts are counted in `Spell_Start` 0x4152A0 (ECL op 122 → 0x421280). If the stored checksum is wrong at that moment, all counters of that card are zeroed first.
- Captures and max bonus are counted in `Spell_End` 0x4161B0.
- Neither happens in replay playback.

### 2.6 PSCR (0x178 × 12) — practice scores

In memory at 0x164BB6C + shot·0x178.

| off | type | field |
|---|---|---|
| 0x0C | u32[9][5] | practice plays [stage][difficulty] |
| 0xC0 | u32[9][5] | practice hiscore ÷10 |
| 0x174 | u8 | shot |
| 0x175 | u8 | used (the chunk is only saved if ≠ 0) |

- A practice start increments plays and sets +0x175 (GameMgr_LoadScoreData).
- The practice result (Result mode 0x11) stores max(hiscore, score) at index `[shot + byte 0x164D0B2]`. The byte 0x164D0B2 is 0 on every path that reaches a result screen (VERIFIED, answers_phase7 §3; it is nonzero only during attract demos).

### 2.7 LSNM (0x18)

+0x0C char[12] = last name entered. The default is 8 spaces. It is held in Result+0x46450 and pre-fills both name entries.

### 2.8 FLSP (0x20) — Last Word unlocks

In memory at GM+0xC (0x160F514), +0x0C..+0x1C u8[17]. Slot k holds 0xCD + k when Last Word 205 + k is unlocked, else 0.

The slots are recomputed by `LastWord_ComputeUnlocks` 0x46CBBB when the spell-practice card list opens. Card ids count from 0. "Prac" = practice capture counters (CATK +0x1F4[12]); "story or prac" = either capture counter.

| card | owner | unlock condition (VERIFIED) |
|---|---|---|
| 205 | Wriggle | ≥ 2 shots have a no-continue 0x4000 (Final B) clear on any of E/N/H/L |
| 206 | Mystia | ≥ 3 such shots |
| 207 | Keine | ≥ 50 distinct cards captured in practice |
| 208 | Reisen | ≥ 4 such shots (as 205) |
| 209 | Eirin | card 137 captured (story or prac) |
| 210 | Kaguya | ≥ 15 cards of the 43-card "boss last spell" list (0x4C6B90, test 0x414540) captured in practice |
| 211 | Mokou | cards 195 and 145 captured (story or prac), and card 204 attempted |
| 212 | Tewi | Last Words 208, 209, 210 all attempted |
| 213 | Keine (Hakutaku) | Last Words 205, 206, 207, 211 all attempted |
| 214 | Reimu | all 49 cards of the Normal list (0x4C61C8) captured with shot 6 (Marisa solo; matches the in-game hint) |
| 215 | Marisa | CLRD[4] (shot 4) no-continue Hard or Lunatic has 0x4000 |
| 216 | Sakuya | ≥ 120 distinct cards captured in practice |
| 217 | Youmu | ≥ 6 shots as 205 |
| 218 | Alice | ≥ 3 shots with Extra no-continue mask & 0x100 (Extra stage cleared) |
| 219 | Remilia | ≥ 30 last-spell-list cards captured in practice |
| 220 | Yuyuko | union entry (12) any-clear Lunatic & 0xC000 ≠ 0 |
| 221 | Yukari | all of 205..220 attempted |

"Attempted" means any attempt counter ≠ 0 (story or practice).

Card lists:

| list | address | count |
|---|---|---|
| Easy | 0x4C6120 | 42 |
| Normal | 0x4C61C8 | 49 |
| Hard | 0x4C6290 | 50 |
| Lunatic | 0x4C6358 | 50 |
| Extra | 0x4C6420 | 14 |
| all | 0x4C6458 | 222 |

Pointer table 0x4C67D0, counts at 0x4C67E8. `Spell_DifficultyOf` 0x4144D0 returns the list index, and 5 for ids ≥ 205.

### 2.9 PLST (0x228) — play statistics

In memory at 0x164CD0C (GM+0x3D804).

| off | type | field |
|---|---|---|
| 0x0C | u32[4] | total run time h, m, s, ms (`Supervisor_UpdateRunTime` 0x4482A1, from timeGetTime) |
| 0x1C | u32[4] | total play time h, m, s, ms (`Supervisor_UpdatePlayTime` 0x448418) |
| 0x2C | u32[7][17] | counters: block 0..4 = difficulty E, N, H, L, Ex; block 5 unused (always 0); block 6 = total |
| 0x208 | u8[32] | music-room "heard" flags |

Inside each 17-word block:

| index | counter | updated in |
|---|---|---|
| [0] | plays | StageStart on a fresh start, and on each continue (continue menu, 0x4037B0) |
| [1..12] | plays per shot | same |
| [13] | retries | StageStart when scene == 10 |
| [14] | clears | ending 0x429980 / Extra clear in GameMgr_Calc |
| [15] | continues | continue menu |
| [16] | practice plays | StageStart in practice |

- StageStart counts only when not replay, not spell practice and not slow mode. Every counter is capped at 999999.
- Music flags are set by `Music_PlaySlot`/`Music_PlayFile` unless in replay or demo. The ending sets flag 18, and 19 on a good ending.
- The parser checks block 6 == Σ blocks 0..5.

### 2.10 VRSM (0x1C) and the exe check

| off | type | field |
|---|---|---|
| 0x0C | char[8] | version tag "0100d" |
| 0x14 | u32 | exe size |
| 0x18 | u32 | exe sum = Σ u32 words of th08.exe, count = size/4 − 1 (0x443730) |

`ExeCheck` 0x44858D passes when any of these hold:
- no file `th08_%.4x%c.ver` exists;
- the tag is "debug";
- a line "tag size sum" in that file matches.

The same check is applied to replays (§3.7).

### 2.11 Slowdown % (stored in HSCR and replays) — VERIFIED

Accumulators: A (0x17CE8E8) and B (0x17CE8EC). Each FPS measurement (window ≥ 500 ms) during gameplay adds:
- B += 60;
- A += 60·w, where w = 1.0 if fps > 54, 0.8 if > 42, 0.6 if > 30, else 0.5.

At the result: x = clamp((A/B − 0.5)·2, 0, 1), and slowdown % = (1 − x)·100.

### 2.12 Hidden unlock code — VERIFIED (0x4550FC, result-viewer difficulty page)

While holding Focus or Skip, enter Right ×4, Left ×1, D ×2, Q ×3.

Effect:
- every CLRD mask becomes 0xFFFF;
- every card's attempt counter +1;
- plays SE 0x1C.

---

## 3. Replays (`replay/th8_NN.rpy`) — VERIFIED

### 3.1 Runtime object

The object is 0xDC bytes, pointer at 0x18B8A28. `Replay_Register(mode)` 0x451F90 sets g_Input and g_InputPrev to 0.

| mode | calc entries |
|---|---|
| 0 record | `Replay_RecordCalc` 0x452310, prio 17. On-register `Replay_RecordStageStart` 0x452830; on-remove 0x453080. Also `Replay_FrameHook` 0x4522A0, prio 7 (11 §1.4). |
| 1 play | `Replay_PlayCalc` 0x452550, prio 6. On-register `Replay_PlayStageLoad` 0x452D60. Also `Replay_FastForward` 0x452490, prio 18. If header+6 (debug flag) ≠ 0, `Replay_PlayCalcDebug` 0x4526C0 is used instead: 6-byte frames {input u16, flag u16, RNG state u16}, the RNG state is forced every frame. Retail files have 0. |

Object fields:

| off | field |
|---|---|
| +0 | frame counter |
| +4 | clear-bonus frame counter |
| +8 | info pointer (0x134-byte header block) |
| +0x50 | input cursor |
| +0x54[9] | input end per stage |
| +0x78 | stream start |
| +0xA0 | fps cursor |
| +0xA4[9] | fps end per stage |

### 3.2 Recording, per frame (calc prio 17, after the game objects)

Only while flags bit 2 (gameplay running):
1. `g_InputPrev = g_Input; g_Input = raw input`.
2. If slow mode (cfg+0x25) or Supervisor+0x1A4 bit 3 is set, nothing is recorded. VERIFIED: no retail code sets bit 3, except the anti-tamper fill that writes 0xFFFFFFFF over the whole Supervisor (answers_phase7 §2).
3. While flags bit 9 (stage-clear bonus): if counter+4 > 2, return; else counter+4 += 1. So at most 3 frames are recorded during the bonus.
4. `p += 2; end[stage] = p + 2; *(u16*)p = raw`.
5. If `frame % 30 == 0`:
   - `fps[q] = fps_now | (flag ? 0x80 : 0)`, `fps[q+1] = fps_now`;
   - `fpsEnd[stage] = q + 2`; `q += 1`.

   fps_now is u8 0x17CE8F0. The flag is the u32 0x17CEA90, = Supervisor+0x338; its only writer stores 0 (MainMenu_OnRegister), so it is always 0. VERIFIED (answers_phase7 §1).
6. `frame += 1`.

The game objects ran earlier in the same frame using `g_Input` from the previous frame. So the input of frame n is the raw input sampled at the end of frame n−1.

Stage start (`Replay_RecordStageStart`):
- Allocates two 0xD2F00-byte buffers (record, fps).
- Fills the 0x24-byte record header (§3.4).
- Sets the cursor to record+0x24 and writes a 0 word there.
- Also writes 0 at +0x26 and the RNG state at +0x28. These are debug-format leftovers; they are overwritten by frames 1 and 2.

So the stream is `[dummy 0][frame 1][frame 2]…`.

Before allocating, the previous stage's record +0 is set to the current score (stats+8). Record +0 is therefore the **score at the end of that stage**. For the last stage it is set when the replay is finished.

`Replay_AppendEnd` 0x4531A0 runs on game end, before saving: `p += 2; *p = 0; end = p + 6`. The stream ends with a 0 terminator plus 4 uninitialised bytes. The demo files contain 0xCDCD there.

### 3.3 Playback, per frame (calc prio 6, before the game objects)

Only while flags bit 2, not slow mode, and **not** flags bit 9:
- `g_InputPrev = g_Input; g_Input = *p; p += 2`.
  - The first read is the dummy 0 word. This matches the one-frame latency of recording, so frame n sees the same value it saw when recorded.
- Key repeat is emulated (normally done by the input poller):
  - repeat flag 0x164D538 = 0;
  - if input == previous: when counter 0x164D53C > 29, repeat = ((counter & 7) == 0), and if counter > 37 it is reset to 30; then counter += 1;
  - else counter = 0.
- If `frame % 30 == 0`:
  - b = fps[q+1]; displayed fps = b & 0x7F; red flag 0x17CEA94 = b >> 7; q += 1.
  - QUIRK: playback reads byte q+1, so it shows the sample one step ahead (and the first byte is never read). The record's overlapping two-byte writes make the stream one byte longer than the sample count. Reproduce it exactly if bit-exact display matters.
- `frame += 1`.

Frames during the stage-clear bonus (bit 9) are not read. The ≤ 3 frames recorded there sit after the frames used and are harmless.

Playback speed byte 0x164D0BD (chosen in the replay menu, §3.8):

| value | behaviour |
|---|---|
| 0 | normal |
| 1 | "slowdown reproduction". GameMgr_Calc runs a frame only on a pattern that depends on the stored fps: fps < 20 → 1 of 3 frames; 20–29 → 1 of 2; 30–39 → 2 of 3; 40–49 → 5 of 6; ≥ 50 → every frame. |
| 2 | fast. `Replay_FastForward` (prio 18) returns 6 ("restart the calc chain") on every frame except `frame % 5 == 4`, only when no boss is active (0x42F1F0). |

In any playback mode, while skippable dialogue is shown (HUD impl+0x22D7C), the chain is restarted on 2 of every 3 frames (= 3× speed).

The HUD shows the playback fps in yellow (0xFFFFFFD0), or red (0xFFFF4040) when the flag is set.

### 3.4 Stage record (0x24 header + streams)

See 11 §6 for the table. Corrected meaning of +0: score at the **end** of that stage (÷10). Playback (`Replay_PlayStageLoad`) restores score, lives, bombs, power, rank, seed and so on from the record of the stage being started (11 §3.4).

### 3.5 File layout

```
0x000  file header (0x68), partly encrypted
0x068  LZSS-compressed, encrypted body
size   "USER" section (plain text; not covered by checksum or encryption)
```

File header:

| off | type | field |
|---|---|---|
| 0x00 | char[4] | "T8RP" |
| 0x04 | u16 | version 6 |
| 0x06 | u8 | debug-frame flag (0) |
| 0x07 | u8 | 0 |
| 0x0C | u32 | file size without the USER section (= 0x68 + compressed size) |
| 0x10 | u32 | checksum = 0x3F000318 + Σ bytes [0x15 .. 0x68) + Σ compressed bytes, taken **before** encryption |
| 0x14 | u8 | random byte |
| 0x15 | u8 | encryption key = random + 0x40 |
| 0x18 | u32 | compressed size (encrypted from here on) |
| 0x1C | u32 | decompressed size (= image size − 0x68) |
| 0x20 | u32[9] | file offset of each stage's record (0 = stage not played), in decompressed-image coordinates |
| 0x44 | u32[9] | file offset of each stage's fps stream |

Encryption runs over bytes 0x18..0x67 of the header, then over the compressed body: `b += key; key += 7` (u8 arithmetic). Decrypt by subtracting the same sequence.

Saving uses **3 RNG draws** (Rng_Range16(256) each): the key byte, info +0x68, and header +0x14. (In the decompiled save these are three separate draws; 11 §1.5 is corrected.)

Decompressed image = header (0x68) + info block (0x68..0x134) + for each recorded stage, in order: its record (0x24 header + input words up to `end`), then its fps bytes up to `fpsEnd`.

Info block (offsets in the image):

| off | type | field |
|---|---|---|
| 0x68 | u8 | random |
| 0x69 | u8 | 100 (set at record start; VERIFIED never read) |
| 0x6A | u8 | shot |
| 0x6B | u8 | difficulty |
| 0x6C | char[6] | date "%m/%d" |
| 0x72 | char[9] | name (pre-set "NO NAME"; the entered name on save) |
| 0x7B | u8 | practice flag (GameMgr_IsPractice) |
| 0x7C | i16 | spell id in spell practice, else −1 |
| 0x7E | char[48] | card name (spell practice) |
| 0xAE | u16 | 0x100 (demo files 1–3, tag "debug", have 0) |
| 0xB0 | u32 | final score ÷10 |
| 0xB4 | 60 B | th08.cfg copy at record start (+0xD9 = slow mode; must be 0) |
| 0x114 | f32 | slowdown % + 1.12 (constant 0x4B7088) |
| 0x118 | f32 | slowdown % |
| 0x11C | u8 | clear status at record time: 2 if any no-continue Final B clear on E–L for this shot or shot ≥ 4; else 1 if any Final A clear; else 0 |
| 0x120 | u32 | 30 (fps sample interval) |
| 0x124 | u32 | exe size |
| 0x128 | u32 | exe sum |
| 0x12C | char[8] | version tag "0100d" |

USER section:
- "USER", u32 total length (including this 12-byte head), u8 type 0, 3 garbage bytes.
- Then Shift-JIS lines `key\tvalue\r\n`:
  - プレイヤー名 (name)
  - プレイ時刻 (`%Y/%m/%d %H:%M:%S`)
  - キャラ名
  - スコア (`%d0`)
  - 難易度
  - 最終ステージ (stage name, or "Clear"), or in spell practice カード名 `No.%3d %s`
  - ミス回数
  - ボム回数
  - 処理落ち率 (`%f%%`)
  - 人間率 (`%3.2f％`, from HSCR+0x84 / 100)
  - ゲームのバージョン (`%d.%.2d%c`, 1,0,'d')
- NUL-terminated, padded to an even length.

### 3.6 Save flow (`Replay_Finish` 0x4531F0, ECX = path, EDX = name)

The save is skipped (nothing written) when any of these hold:
- playing back;
- a normal game (not practice) on difficulty < 4 whose current config differs from the recorded one;
- the recorded slow mode ≠ 0.

Otherwise:
1. Fill date, name, score, slowdown and card name.
2. Draw 3 RNG values.
3. Build the image and compress it.
4. Compute the checksum, then encrypt.
5. Write the header, body and USER section.

Save UI, Result states 10–14 (0x45696F):

| state | behaviour |
|---|---|
| 10 | "Save replay?" Yes/No. At frame 10: slow mode → message (interrupt 0x13), state 11; continues ≠ 0 → "cannot save" (interrupt 14), state 11. |
| 11 | Message; leaves after 20 frames. |
| 12 | Slot list `./replay/th8_%.2d.rpy` 01..15 (creates the directory `replay`). The cursor moves from frame 20. |
| 13 | Name entry (16 §2.2: same 96-char grid, 8 chars max), pre-filled from LSNM. |
| 14 | "Overwrite?" Yes/No. |

Choosing an existing valid file goes to 14, otherwise to 13. Finishing the name: write the file, store the name in LSNM, leave.

File names: the game writes only `th8_01.rpy`..`th8_15.rpy`. It also lists `th8_ud????.rpy` (user-renamed files).

### 3.7 Load (`Replay_Load` 0x451D90)

Steps:
1. Check the magic "T8RP" and version 6.
2. Decrypt from 0x18.
3. Verify the checksum.
4. Decompress.
5. Reject if info+0xD9 (recorded slow mode) ≠ 0, or if the exe check (§2.10, using +0x124/+0x128/+0x12C) fails.
6. Convert stage offsets to pointers.

Error message: "リプレイデータが異常".

### 3.8 Replay menu (`MainMenu_Replay` 0x46E136, draw `MainMenu_ReplayDraw` 0x46EF8D)

Listing:
- Loads slots 01..15, then up to 45 `th8_ud????.rpy` files (shown as "User ").
- 15 rows per page; Left/Right moves ±15 rows.
- Row format: `"%s %8s  %6s %7s  %8s"` = No., name (+0x72), date (+0x6C), character (table 0x4C86C0[shot]), difficulty (0x4C86F0[diff]). For spell practice the last column is `Spell No.%3d`.

Detail view after choosing a file:
- slowdown `%2.3f%%`;
- a "Stage    LastScore" table: `%s %9d0` per stage that has a record, `----------` otherwise. For spell practice: `No.%.3d %9d0`.

The user then:
- picks one of the recorded stages (sub-state 2);
- picks the speed (sub-state 3): 3 options (normal / slowdown reproduction / fast), or 2 in spell practice.

Start:
- flags |= 8 (replay);
- the path is copied to 0x164D0C4;
- difficulty, shot and spell practice (spell id ≥ 0 → flags bit 14, id → 0x164D0B8) come from the header;
- stage = the chosen stage;
- scene 2.

Demo play uses the same loader with `demo/demorpy%d.rpy` (12 §2.4).

### 3.9 Parser results (VERIFIED)

`tools/rpy.py` checks:
- magic and version;
- checksum;
- sizes;
- stage and fps offsets (the first one is 0x134);
- the dummy word;
- the terminator;
- input bits ⊂ 0x1FFF;
- info constants;
- the USER length.

All 4 demos pass. They contain one stage each:
- demo0: Lunatic, Reimu & Yukari, stage 5, 11083 frames, tag "0100a";
- demo1–3: stages 4A / 3 / 2, tag "debug".

The fps bytes match 30-frame sampling: 371 bytes ≈ 11083/30 + 2.
