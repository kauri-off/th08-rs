# Spec coverage map (phase 7; implementability pass RE-Q23)

This is the whole game, feature by feature, mapped to the spec that describes it.

Status:
- **COVERED**: the spec is enough to implement the feature, with every gameplay-relevant value VERIFIED.
- **PARTIAL**: the behaviour is specified, but listed details (layout, pixels, a sub-feature) are missing or
  INFERRED.
- **MISSING**: no spec yet.

Addresses are th08.exe VAs.

## A. Implementability (RE-Q23): can an implementer write it from the numbered specs alone?

Rating per alpha feature. COVERED = logic, order, layout/positions (or the ANM scripts that hold them), sprite
ids, sounds and file effects are all in a numbered spec; PARTIAL lists exactly what is missing.

| alpha feature | specs | rating |
|---|---|---|
| Boot, logo, loading screens | 12 §1, 11 §3 (loading frame count) | COVERED |
| Title / main menu (top, difficulty, character, practice select, demo start) | 12 §2.1–2.5, 11 §4 (demo), 16 §4 (unlock notices) | COVERED |
| All stages 1–6 (A/B) + Extra: STD backgrounds, ECL enemies/bosses/natives, MSG dialogue, BGM | 06, 07 (incl. §10.3a, §12.1), 10, 02, 08, 13 | COVERED |
| Player, shots, bombs, last spells, options, gauge, time orbs | 09, 14, 08 §8, 11 §5, §8 | COVERED |
| HUD, popups, boss UI, spell plates, clear panel | 12 §5, 13 §4 | COVERED |
| Pause menu (logic, draw, capture background) | 12 §3 | COVERED |
| Continue prompt, game over (logic, draw, continue sequence) | 12 §4, 11 §4.2 | COVERED |
| Result screen + name entry + ranking + end panel | 16 §2.1–2.3, §2.5 | COVERED |
| Replay save dialogue / replay file write | 16 §2.6, 15 §3.2, §3.5–3.6 | COVERED (LZSS encoder free choice, §1 row) |
| Replay menu / load / playback (speeds) | 12 §2.8, 15 §3.3, §3.7–3.8 | COVERED |
| score.dat (all chunks, load/save, unlocks) | 15 §2 | COVERED |
| th08.cfg + option menu + key config | 15 §1.1–1.3, 12 §2.7 | COVERED (pad-button assignment needs a joystick; rule in 15 §1.3) |
| Endings + staff roll | 16 §1, §3 | COVERED |
| Music room | 16 §5 | COVERED for WAV mode; **PARTIAL**: MIDI mode (cfg +0x1F = 2) playback is MISSING (§2 row) — in WAV mode nothing is missing |
| Spell practice + Last Word | 12 §2.6, 16 §6, 11 §3.2 | COVERED |
| Stage practice | 12 §2.5, 11 §3.2, 15 §2.6 | COVERED |
| Result viewer (main menu "Result"), score.txt | 16 §2.4, §2.7 | COVERED |

**PARTIAL / MISSING (complete list):**
- MISSING: MIDI music mode (SMF player 0x443C60–0x445440: timing, loop, fade, volume, `init.mid`). Only reachable when
  the option BGM = Midi or `thbgm.dat` is invalid.
- PARTIAL: LZSS encoder (any valid stream decodes; byte-identical score.dat/replays not required, INFERRED).
- PARTIAL by nature: GDI text pixels (10 §6; a 7/14 px monospace font reproduces the layout).
- PARTIAL: Screenshot 16-bit back-buffer branch (00 §2; 24-bit path specified).
- Not needed: window/platform code, DirectSound internals, anti-tamper paths (only reachable after tampering).

**Remaining INFERRED items per spec** (after RE-Q26; every item that could affect sync, RNG order, the draw log or a
saved file was verified or corrected — changelog/answers_re_q26.md §2. What is left is naming, purpose or platform):
- 00: back-buffer format choice (16-bit); subsystem / scene-id names; game coordinates wording for non-bullet objects.
- 01: reserved table field; `out_size` includes the 4 wrapper bytes (no caller depends on it).
- 02: MIDI fade argument and MIDI-only paths; DirectSound chunk-size meaning; BGM slot 1/2 role names; spell-practice
  BGM table field names.
- 03: texture resource priority meaning (D3D `SetPriority`, no rendering effect); reduced-colour option name.
- 04: character name order; loading-picture `h`/`a` variants; `a`/`as` sht naming; `face_*sp`, `face_cdbg`, `msg4dm`,
  `.ver` roles.
- 07: names of flag A bits 23/25/30/31.
- 09, 14: per-bomb choreography "detail level" note (superseded by 14 §2).
- 10: GDI text pixel equivalence.
- 11: difficulty / GameFlags bit 4 names; purpose of slowdown replay playback.
- 12: cause of the empty boot presents 3–5 (the observable is VERIFIED); front.anm VM roles by position.
- 13: SE buffer caps bit; unused SE ids.
- 15: joystick-only bits after a failed keyboard `GetDeviceState` (platform).

## 0. Method and numbers

`re/tools/coverage.py` compares the call graph (direct calls plus every function address found as a pointer
in code or data) with the addresses and symbol names cited in `specs/*.md`.

Layout of th08.exe .text:
- game code: 0x402000–0x476CE0;
- import thunks, then statically linked **D3DX8** (0x476D00–0x4A3000; compiler banners at 0x400613…);
- **CRT** after that.

D3DX8 and the CRT are library code. A port replaces them, so they are out of scope (D3DX behaviour that
matters is written into the specs where used, e.g. 10 §6 TRIANGLE filter).

Game range: 1116 functions, 478 KB. All of them are reachable.

| class | functions | bytes |
|---|---|---|
| cited directly by a spec | 496 | 369 KB (77 %) |
| uncited, but every caller is cited (helpers whose behaviour is folded into the caller's spec) | 367 | 64 KB |
| uncited, with uncited callers | 253 | 45 KB, listed in §3 |

After COVERAGE.md and changelog/answers_phase7.md were added (they cite the cluster addresses), a re-run gives:
590 cited (418 KB, 87 %), 376 helpers (43 KB), 150 orphan functions (18 KB). The table above is the state
before this phase.

Most of the 45 KB is covered by behaviour, not by address: ECL natives 07 §12, effect update functions
13 §2.6, bullet ex-command handlers 08 §5. Real gaps are in §3.

## 1. Engine and platform

| feature | spec | status | main functions |
|---|---|---|---|
| Archive th08.dat (XOR, LZSS, edz) | 01 | COVERED | `PbgArchive_Open` 0x4748C0, `_ReadHeader` 0x474CE0, `_ParseTable` 0x474FA0, `Lzss_Decompress` 0x4740E0, `Edz_DecryptFile` 0x43E390, `FileSystem_OpenFile` 0x43E660 |
| LZSS **compressor** (score.dat, replays) | 15 §2.1/§3.5 (format only) | PARTIAL: the encoder is not specified. Any valid LZSS stream decodes, so byte-identical files are not needed (INFERRED). | 0x473CB0 (+0x474450…0x474760), 0x475270 |
| Main loop, 60 Hz deadline, frame skip | 00 §2, 15 §1.1 (+0x23) | COVERED | `WinMain` 0x4418C0, `Frame_Run` 0x441E70, `Frame_Present` 0x442060 |
| Task system (funcChain) | 00 §3–4 | COVERED | `FuncChain_*` 0x43C760…0x43CF50 |
| Supervisor scenes | 11 §7, 00 §4 | COVERED | `Supervisor_Calc` 0x445453, `_OnRegister` 0x445FA3, `_Init` 0x4464C8 |
| Timers, game speed | 05 §12.6, 00 §2 | COVERED | `Timer_*` 0x4065F0…, `Supervisor_AdvanceTimer` 0x447421 |
| D3D device, render states, camera | 00 §1, 05 §12.4–12.5, 06 §10 | COVERED | `Window_InitD3D` 0x4424C0, 0x442F60 |
| Window, cursor, single instance, log.txt, desktop shortcut check | none | MISSING, not needed for the port (platform) | 0x442240, 0x442390, 0x443420, 0x4436A0, 0x443840, 0x4439C0, 0x442A80 |
| Input: keyboard | 12 intro (bit map 0x43D970), 10 §5 | COVERED | `Input_Poll` 0x43D970 |
| Input: joystick, shot-focus toggle | 15 §1.1 "Joystick mapping", §1.3 key config | COVERED | `Input_PollJoystick` 0x43D120, 0x43D7E0 (key-config capture) |
| Replay-substituted input, key repeat | 15 §3.3, 12 intro | COVERED | `Replay_PlayCalc` 0x452550 |
| FPS meter, slowdown % | 15 §2.11, §3.3 | COVERED | 0x446F53 |
| Screenshots | 00 §2 | COVERED (24-bit path; the 16-bit back-buffer branch is not decoded) | `Frame_Present`, 0x44748F |
| Post-present surface capture (pause background etc.) | 12 §1 (surface slot 8), 12 §3 (texture capture: request, callers, rects) | COVERED | 0x406A30, 0x443B60, 0x466F20, 0x467180 |
| Anti-tamper | 11 §4 step 7, 15 §2.2 | COVERED (ignore) | `GameMgr_IsTampered` 0x40BB80 |
| Floating-point environment | 05 §10, 11 §1.2 | VERIFIED at runtime (changelog/answers_phase8.md §1): main thread CW 0x007F (24-bit precision, round to nearest) after CreateDevice; loader threads 0x027F | — |

## 2. Graphics and audio subsystems

| feature | spec | status | main functions |
|---|---|---|---|
| Images, ANM file format | 03 | COVERED | `Anm_*` 0x465AC0…0x4661B0 |
| ANM VM | 05 | COVERED | `AnmVm_Run` 0x45EA00, `AnmVm_SetSprite` 0x45E430 |
| Sprite drawing paths | 05 §9, 06 §11 | COVERED | `AnmManager_Draw2D*` 0x462FF0…, `_Project*` 0x4639E0/0x463D60, `_Draw3DWorld` 0x464470 |
| Text rasteriser (GDI) | 10 §6 | PARTIAL by nature: pixels depend on the MS Gothic GDI rasteriser. An equivalent font is acceptable (INFERRED). | `Text_Render` 0x466310, `Text_RenderToVm` 0x4663B0, 0x43F9C8 / 0x43FCE9, DIB helpers 0x43F10D / 0x43F31C / 0x43F898 |
| Stage backgrounds (STD) | 06 | COVERED | `Stage_Calc` 0x407400, `Stage_DrawLayer` 0x40A1B0, `Stage_DrawHigh` 0x409640 |
| Fog | 06 §8 | COVERED | `Supervisor_FogOn/Off` 0x4481DF / 0x448229 |
| Effects (66 types, ring renderer) | 13 §2 (§2.6a–c particles, orbit, list-B helper) | COVERED | `EffectMgr_*` 0x426xxx–0x428xxx, table 0x4C6D30 |
| Screen effects (flash, fade, shake, slowdown) | 13 §3 | COVERED (per-type handlers 0x45B760…0x45BF10 by behaviour) | `ScreenEffect_Register` 0x45B8B0 |
| Spell-card background, natives 3/6/20 mask | 13 §5, 07 §12.1 | COVERED | `Stage_SpellMaskCallback` 0x4235A0 |
| Sound effects | 13 §1 | COVERED | `Sound_LoadSE` 0x45D3F0, `Sound_Play` 0x45D550, `Sound_Update` 0x45D790 |
| BGM: WAV streaming (thbgm.dat) | 02 | COVERED (DirectSound streaming internals 0x4718D0…0x473790 are platform) | `Music_Play` 0x447D56, `Music_PlaySlot` 0x447E47, `Music_PlayFile` 0x447F21, `Music_Fade` 0x4480F8, `Bgm_*` 0x45C6B0…0x45D2D0, `WaveStream_*` 0x4733E0 / 0x473530 |
| BGM: **MIDI mode** (cfg +0x1F = 2) | 02 (mentions only) | **MISSING**: SMF player, timer, loop, fade, volume, init.mid | 0x443C60…0x445440 (midiOut*, timeSetEvent; main 0x444820 / 0x444A90 / 0x445340, open 0x4442E0, play 0x444620, stop 0x444680, fade 0x4447D0) |

## 3. Gameplay

| feature | spec | status | main functions |
|---|---|---|---|
| Game manager, new game, stage start, routing (4A/4B, 6A/6B) | 11 §2–4 | COVERED | `GameManager_StageStart` 0x43ABD7, `GameMgr_Calc` 0x439BC7, `Stage_Advance` 0x43C4B3 |
| RNG | 11 §1 | COVERED | `Rng_Next16` 0x43ECC0 … |
| Stage scripts / enemies (ECL) | 07 | COVERED | `Ecl_Run` 0x4184B0, `EclTimeline_Run` 0x42A8A0, `EnemyMgr_Calc` 0x42C660 |
| ECL natives 0–31 | 07 §12, §12.1 | COVERED | table 0x4C6CB0 |
| Familiars (enemy, native 8 / chains) | 07 §10.5, §12 | COVERED | `EclNative_8` 0x4244F0 |
| Bosses, spell cards, capture/bonus, Last Spell | 07 §10.2–10.3a, §11, 13 §5; name plates / cut-ins / bonus line 12 §5.8; last-spell flags and dissolve spell 11 §2.1, 14 §2.17 | COVERED | `Spell_Start` 0x4152A0, `Spell_End` 0x4161B0, `Spell_Calc` 0x416B90, `Spell_Draw` 0x4178C0 |
| Bullets, lasers, cancels | 08 §1–7 | COVERED | `BulletMgr_Calc` 0x431240, `PlaceBullet` 0x42F5F0, `Bullet_ApplyExCmds` 0x42FFC0, laser 0x430F20 |
| Items, point value, extends, power | 08 §8, 11 §5 | COVERED | `ItemMgr_Spawn` 0x4400A0, `ItemMgr_Calc` 0x440500 |
| Player: movement, focus, shots, death, deathbomb | 09 | COVERED | `Player_Move` 0x44AEC0, 0x450F60, `Player_Die` 0x44AB40, `Player_DamageAgainst` 0x451670 |
| Team switching (human/youkai focus), solo shot types | 09 §3, 04 | COVERED | `Player_Move`, `g_ShotType` 0x164D0B1 |
| Player options (familiars) | 14 §3 | COVERED | `Option_*` 0x44E3A0…0x44F5E0 |
| Bombs, Last Spells, dissolve | 14 §1–2 | COVERED | table 0x4C7AD0, `Bomb_Start` 0x40BE30 |
| Human/youkai gauge | 09 §9.3, 11 §5 | COVERED | `GameMgr_GaugeAdd` 0x43C0BB, `Gauge_*` 0x406D10… |
| Time orbs, night clock, time system, bad-ending clock | 08 §8, 09 §4.5, 11 §8, 16 §1.1 | COVERED | `Item_CollectTimeOrb` 0x4412B0, `GameMgr_AddTime` 0x418220 |
| Rank | 07 §9.2, 11 §3.2 | COVERED | `EnemyMgr_TimeRank` 0x42C3B0 |
| Extra stage, unlock | 11 §3, 15 §2.4, 16 §4 | COVERED | `Clear_ExtraUnlocked`, `MainMenu_OnRegister` 0x470A6C |
| Dialogue (MSG) | 10 | COVERED (text pixels: see Text rasteriser) | `Msg_Start` 0x43396D, `Msg_Run` 0x433DB3, `Msg_Draw` 0x43542B |
| HUD, side panel, boss UI, popups, clear panel | 12 §5, 13 §4 | COVERED | `Hud_Update` 0x435900, `Hud_DrawPanel` 0x43625D, `Hud_DrawBoss` 0x43741D |
| Pause menu | 12 §3 (logic + draw) | COVERED | `PauseMenu_Update` 0x4037B0 |
| Continue / game over | 12 §4 (logic + draw), 11 §4.2 | COVERED | `ContinueMenu_Update` 0x404890, `ContinueMenu_Draw` 0x4052B0 |
| Stage practice, spell practice, Last Word | 12 §2.5–2.6, 16 §6 | COVERED (logic and draw layout: 12 §2.5, 16 §6.3–6.5) | `MainMenu_PracticeStage` 0x46ADB0, `_SpellPracticeChar` 0x46B174, `_SpellPracticeCard` 0x46BBC0, `LastWord_ComputeUnlocks` 0x46CBBB |
| Demo / attract mode | 12 §2.2, 11 §2, §4, specs/data/README.md (traces) | COVERED | `MainMenu_Top` 0x4674E0, `GameMgr_Calc` |
| Replay record/playback/speed | 15 §3 | COVERED | `Replay_*` 0x451D90…0x4531F0 |

## 4. Front end and files

| feature | spec | status | main functions |
|---|---|---|---|
| Boot logo, loading screens | 12 §1 | COVERED | `Supervisor_OnRegister` 0x445FA3, `Supervisor_DrawLoading` 0x445BD4, `Loading_*` 0x448790… |
| Main menu: top, difficulty, character, option, key config | 12 §2, 15 §1.3 | COVERED | `MainMenu_Calc` 0x467399, `MainMenu_Top` 0x4674E0, `_Difficulty` 0x46A071, `_Character` 0x46A625, `_Option` 0x46837B, `_KeyConfig` 0x469636 |
| Main menu draw of the practice-score, spell-card-record and replay lists | 12 §2.5, §2.8, §2.9 (ASCII queue + highlight glyphs), 16 §6.3–6.5 | COVERED (MOPUP-1; refs `specs/data/drawlog/menu_mopup1.*`, `menu_ref/mopup1_*.png`) | draw 0x47087F → `MainMenu_PracticeStageDraw` 0x46F429, `MainMenu_SpellPracticeCharDraw` 0x46F5CB, `MainMenu_SpellPracticeCardDraw` 0x46FFC0, `MainMenu_ReplayDraw` 0x46EF8D, `MainMenu_CharacterDraw` 0x47052D (12 §2.4), `Menu_DrawPie` 0x46FDC2 / `AnmManager_DrawFan` 0x464EC0 (pie charts, not a rotating graphic) |
| Replay menu | 15 §3.8 | COVERED | `MainMenu_Replay` 0x46E136, `MainMenu_ReplayDraw` 0x46EF8D |
| Unlock notices (info00–02) | 16 §4 | COVERED | `Info_Show` 0x471181 |
| Music room | 16 §5 | COVERED | `MusicRoom_*` 0x449254…0x449C0A |
| Endings, staff roll | 16 §1 | COVERED | `Ending_Init` 0x429980, `Ending_RunScript` 0x428B80, `Ending_Draw` 0x4298F0, `Ending_DrawFade` 0x428910 |
| Result: name entry, ranking | 16 §2.1–2.3 | COVERED | `Result_Calc` 0x4584B0, `Result_NameEntry` 0x45621E, `Result_Draw` 0x4586B4 |
| Result viewer pages (ranking, card history, play data), end-of-game panel, name entry, replay-save dialogue | 16 §2.2–2.6 | COVERED (MOPUP-1) | `Result_TopMenu` 0x454CB2, `Result_CheatUnlock` 0x4550FC, `Result_CharSelect` 0x45567D, `Result_Ranking` 0x455925, `Result_CardGroup` 0x455A33, `Result_CardChar` 0x455CB0, `Result_CardList` 0x455F6B, `Result_PlayStats` 0x4578AA, `Result_DrawEndPanel` 0x457E00, `Result_Draw` 0x4586B4, `Result_SaveReplay` 0x45696F |
| score.txt dump (D key in result viewer) | 16 §2.7 | COVERED (MOPUP-1; sample `specs/data/drawlog/menu_mopup1.score.txt`) | `Result_DumpScoreTxt` 0x454298 |
| th08.cfg | 15 §1 | COVERED | `Config_Load` 0x4477B1 |
| score.dat (all chunks), unlocks, hidden code | 15 §2 | COVERED | `ScoreDat_Load` 0x45A5E0, `ScoreDat_Save` 0x453D0D, `ScoreDat_Get*` 0x45AAC0… |
| Replay file format | 15 §3.5 | COVERED | `Replay_Load` 0x451D90, `Replay_Finish` 0x4531F0 |
| Play time / run time counters | 15 §2.9 | COVERED | `Supervisor_UpdateRunTime` 0x4482A1, `_UpdatePlayTime` 0x448418 |

## 5. Uncited clusters with no cited caller (game range)

The list is ranked by size. "Behaviour covered" means the spec describes what the code does without
quoting its addresses.

| range | size | what it is | status |
|---|---|---|---|
| 0x475270 | 6.8 KB | LZSS encoder (writer side) | PARTIAL, not needed (see §1) |
| 0x443C60–0x445440 | ~5 KB | MIDI player | MISSING |
| 0x4718D0–0x473790 | ~6 KB | DirectSound buffers, streaming thread, sound-manager ctor (0x45C190 / 0x45C4B0, SetTimer) | platform; behaviour covered by 02 / 13 |
| 0x423400–0x425300 | ~5 KB | ECL natives 2, 4, 7, 8, 9, 11, 16, 21, 25, 27, 28 | behaviour covered, 07 §12 |
| 0x46F429–0x47052D, 0x46FDC2, 0x464EC0 | ~5 KB | main-menu list draws | COVERED |
| 0x45567D–0x455F6B, 0x457E00 | ~3.6 KB | result-viewer states | COVERED |
| 0x45B760–0x45BF10 | ~1.9 KB | screen-effect type handlers | behaviour covered, 13 §3 |
| 0x443420, 0x443840, 0x4436A0, 0x442390, 0x442A80, 0x446A37, 0x446D06, 0x446232 | ~4 KB | start-up, window proc, DirectInput init, Supervisor teardown, timer resolution | platform |
| 0x43F10D–0x43F898 | ~2 KB | text DIB / raster helpers | PARTIAL (10 §6) |
| 0x44748F | 0.7 KB | BMP writer | COVERED (00 §2) |
| 0x426B20…0x427B50, 0x40E120 / 0x40E200 | ~1.5 KB | effect init/update functions | behaviour covered, 13 §2.6 / 14 |
| 0x4322B0…0x4329F0 | ~2 KB | bullet ex-command handlers | behaviour covered, 08 §5.3 |
| 0x449CA0, 0x4071A0, 0x4143E0, 0x42A280, 0x42EE80, 0x409C20, 0x437A40, 0x44DC60 | ~1.8 KB | small ctors/dtors and helpers. Callers are unwind funclets or vtables. | INFERRED irrelevant; check in a mop-up brief |

## 6. What the port still needs from RE

As of RE-Q23 (see §A): only the MIDI music mode is unspecified. Runtime traces, menu/result layouts, score.txt and
the small-helper mop-up are done. Open implementer questions go through `docs/briefs/IMPLn_QUESTIONS.md`.
