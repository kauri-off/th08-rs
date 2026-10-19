# Answers to docs/briefs/IMPL10_QUESTIONS.md (RE side)

Questions covered: **1, 2, 3, 4, 5, 6, 7, 8, 9** (all).

Sources:
- static reading of th08.exe: `Ascii_Calc` 0x402200, `Ascii_DrawPopups` 0x405420, `Popup_Large` 0x403460,
  `Popup_LargeStatic` 0x403600, `Hud_DrawBoss` 0x43741D, `Enemy_KillChildren` 0x42ADB0, `EnemyMgr_Calc`
  0x42C660, `Ecl_Run` (ops 90–92, 176), `EclNative_12`, `Stage_Calc` / `Stage_DrawLow`, `Msg_Start`,
  `EffectMgr_OnRegister`, `GameManager_StageStart`, 0x437F5C, 0x438046, `Spell_Start` 0x4152A0, `Spell_End`
  0x4161B0, `Player_UpdateBomb` 0x44C650, 0x450C50, 0x450840, `Player_AddDamageRect` 0x44DFA0;
- data: `stg5bg.anm`–`stg7bg.anm`, `stage5.std`–`stage7.std`, `ascii.anm`, `front.anm` / `ename.png`;
- runtime ref `specs/data/ref/spell54_boss_under_fire_f209.png` (phase 8 wine probe; no new probe run this phase).

Specs fixed in place: 13 §4 (large popups), 12 §5.4 (boss popup line), answers_impl9 §14 (parent pointer, ÷3
compounding, "+0"), 10 §4 (Msg_Start hooks), 11 §2.1 (bits 7–8, 13), 07 §4.4 flags B bit 7, §12 native 12, op 176,
§11.1 bit 5, 09 §4.3 explosion row. `symbols.csv`: 0x41FD20 renamed `Enemy_HasParent`; `Popup_Large(Static)`
VERIFIED; new 0x418130, 0x418180, 0x437F5C, 0x438046, 0x41EFC0, 0x41F000, 0x42B2F0.

## 1. Large popups — VERIFIED

| aspect | small popups (for comparison) | large popups |
|---|---|---|
| slots | 720 (+3) | 128, round robin (+0x8298) |
| spawn pos | pos + playfield origin | pos + playfield origin (32, 16); `Popup_LargeStatic` also x += count·3.5 |
| per-frame motion | y −= 0.5·gs | **none** |
| life | 60 | 90 (`LargeStatic` starts at age 88 → 2 frames) |
| gating | not paused / continue menu / bullets frozen (bit 10) | same |
| text origin | centred: x − 4·digits | x0 = pos.x − count·3.5 (count = glyphs + 1, +0x31), unscaled |
| glyph order / advance | — | last stored first; advance 7·scale.x |
| anchor | — | top-left (VM +0x548 anchor 0x1800) |
| sprite | age-based variants | 0x88 + glyph, no variants |
| scale | 1 | slot +0x28/+0x2C, copied from manager +0x826C/+0x8270 at spawn |
| alpha | player-distance rule | the same rule (0x50 / ramp / 0xD0); the colour's own alpha is ignored |

So a static popup's text starts exactly at pos.x + 32 and a normal one is centred on pos.x + 32 (± the
count·3.5 offset).

`Hud_DrawBoss`: every frame when not paused, no continue menu, bullets not frozen and boss slot 0
(0xF54CC0) ≠ 0 → manager scale (1, 1) and `Popup_LargeStatic`(pos (2, 29) → screen (34, 45), p3 =
`Enemy_ChainChildCount(boss)`, p4 = boss +0x3380, colour 0xFFF0F00F). There is **no** count > 0 test: the text
is "+0" without children. The runtime ref `spell54_boss_under_fire_f209.png` shows the yellow "+0" at the top
left (~34, 41–47).

## 2. `Enemy_KillChildren(e, reward)` 0x42ADB0 — VERIFIED

Chain links: e+4 = prev, e+8 = next (parent+8 → first child → …); child +0x2DA4 = **parent pointer** (set by
ECL ops 90–92, which also increment parent +0x3380 and append the child at the chain tail). The former name
"drop on parent death" flag was wrong (0x41FD20 = `Enemy_HasParent`).

Chain-kill path (e has children):
1. n = `Enemy_ChainChildCount(e)`; colour once: n < 2 → 0xFFFFFFFF, 2–5 → 0xFFFFFFD0, 6–9 → 0xFFFFFFB0,
   ≥ 10 → 0xFFFFFF80.
2. For each child (via +8): if child flag A bit 9 (follow) → child +0x2D40 := parent pos (+0x2D34); set child flag
   A bit 10 (0x400); +0x2DA4 = 0, +8 = 0, +4 = 0.
3. If reward ≠ 0, for each child k:
   - cancel type = 7 if the parent has flag A bit 1, else 9;
   - orb count c: team shots: n < 8 ? 2n + 10 : 26; solo human: n < 4 ? 6n + 16 : 40; solo youkai: n < 10 ?
     2n + 6 : 26;
   - **QUIRK**: if the player is bombing (0x17D6ED4 ≠ 0) n := n / 3 here, inside the loop, so it compounds
     (n/3, n/9, …) and also lowers the following children's c;
   - `Popup_Large`(child +0x2D88 (the old final pos), n, 0, colour); then child final pos +0x2D88 := +0x2D34 +
     +0x2D40;
   - `Player_AddCancelCircle`(pos, 32.0, 2.0, 8, type);
   - c times: r = `Rng_FloatRange(2c)`, then a = `Rng_SignedRange(π)` (**2 RNG draws**, r first); orb at pos +
     (cos a·r, sin a·r), z 0; `ItemMgr_Spawn`(type 7 time orb, mode 3);
   - if !0x42F1F0(EnemyMgr) or a spell card is active (0x4178A0): child +0x3304 := 8 and `Enemy_DropItems`(child, 0);
   - `Sound_PlayAt`(2 + (k & 1), pos).
4. Always for each child: +0x330C = +0x3308 = 0, +0x3304 = −2.
5. Boss part (reward ≠ 0): manager scale (2, 2); `Popup_Large`(boss +0x2D88, boss +0x3380, 0, 0xFFF0F00F);
   scale (1, 1); then 2·(+0x3380) times: r = `Rng_FloatRange(128)`, a = `Rng_SignedRange(π)` (r first), pos =
   boss +0x2D88 + polar(a, r), z 0, `ItemMgr_Spawn`(type 7, mode 1); then `Player_AddCancelCircle`(boss pos, 32,
   1.0, 16, 7) and `Timer_Set`(0x18B89D4, 0).

Single-kill path: taken when e has a parent and reward ≠ 0 (a chained child killed on its own; `EnemyMgr_Calc`
first does parent +0x3380 −= 1): gauge −= gauge/12; timers 0x18B89E0 = 0, 0x18B89C8 = 30, 0x18B89D4 = 50; final
pos recomputed; `Popup_Large`(pos, 1, 0, 0xFFFFFFFF) ("+1"); `ItemMgr_Spawn`(type 7, mode 1); drops cleared; then
unlinked from the chain (0x42B2F0).

Callers: `EnemyMgr_Calc` death (reward 1), `Enemy_CheckLifeThresholds` (1), `Enemy_CheckTimeout` (0),
`Enemy_Free` (0), `Player_UpdateBomb` dissolve end (0).

## 3. Stage sparkle points — VERIFIED (static bound)

Only `stg5bg`, `stg6bg` and `stg7bg` scripts 3 and 8 set tag 18 (ANM op 25 `set_tag`). Placement (from the STD
instances): stage 6/7 — object 2 has 1 tagged quad, 19 instances; stage 5 — object 2 has 4 tagged quads
(16 instances), object 4 has 5 (15 instances), instance spacing 384. Simulating the camera path with the two
sparkle tests (distance < fog near ≤ 1000; facing window 80 ≤ d ≤ |size|/2 + 960) gives at most **15** points
in one frame; even without the facing test the maximum is 28. So the shipped data never exceeds the 32-entry
array (+0x6480 … +0x6600). Note `Stage_DrawLow` clears only the first 16 entries and the spawn uses 12 (06 §10.2, §11.1);
a port can bound the list at 32 safely.

## 4. Native 12 chain part — VERIFIED

e+8 is the child-chain "next" link (see §2). Native 12 walks from the **calling** enemy's +8:
- var 10001 (ctx+0x1C) == 0: set flags B (+0x3328) bit 7 on every chained child; interrupt 2 to the spell-background
  VMs 0x4E4B68 / 0x4E4E0C;
- else: clear bit 7; interrupt 1.

Flags B bit 7 = "paused": `EnemyMgr_Calc` (0x42C94F) skips the enemy's ECL and holds its script time
(`Timer_Decrement(e+0x2E14)`). The same skip is taken when flag A bit 30 is set and (bombing or player byte
0x17D5EF8 ≠ 0).

## 5. Spell-practice card id, effect file, name plates — VERIFIED

- Card id: GM+0x3DBB0 = 0x164D0B8 (i16); spell practice = GameFlags bit 14.
  0x418130 `GameMgr_IsPracticeCard(id)`, 0x418180 `GameMgr_IsPracticeCardInRange(lo, hi)`.
- `EffectMgr_OnRegister`: spell practice and id ≥ 0xD8 → effect file name from table 0x4C7144[id]; else
  0x4C7480[stage]. It sets g_Stage+0xB34 = 0 and +0xB30 = 2.
- `GameManager_StageStart` in spell practice sets +0xB34 = 2 for stage idx 6 unless id ∈ 0x77–0x7A; idx 7 unless
  id ∈ 0x93–0x96; idx 8 unless id ∈ 0xBF–0xC1 or id = 0xD5.
- Name plate `Hud_SetStageNamePlate` 0x438046 → `Hud_SetBossNamePlate(n)` 0x437F5C (copies `front.anm` sprite n's
  rect into sprite 10 via 0x467040):

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

## 6. `Msg_Start` entry 0 hooks — VERIFIED

| stage idx | effect |
|---|---|
| 5 | plate 0x16 (Reisen) |
| 6 (6A) | g_Stage+0xB34 = 2 → the spell-background VMs use slot-9 scripts 2, 3 instead of 0, 1 |
| 7 (6B) | swap face slots 2/3 (0x4ECC9C ↔ 0x4ECCA0), +0xB34 = 2, plate 0x18 (Kaguya) |
| 8 (Extra) | swap, +0xB34 = 2, plate 0x19 (Mokou) |

Writers of +0xB34: `EffectMgr_OnRegister` (0), `Msg_Start`, `StageStart` (§5). Writers of the count +0xB30:
native 5 and `EffectMgr_OnRegister` (2). `Spell_Start` sets VM i to script +0xB34 + i.

## 7. Last spell: bit 5 and "Last Spell Failed" — VERIFIED

- `Spell_Start`: spell flag bit 5 := GameFlags bit 7.
- GameFlags bit 7 is set by ECL **op 176** (0xB0, `last_spell`), not op 122: clear bits 7, 8, 13; set 7; also set
  13 when (not practice and stage idx 6 or 7) or (practice and id 0x8F–0x92 or 0xAB–0xBE); set flag A bit 30.
- `Spell_End`: captured → if bits 7–8 ≠ 0 they become "bit 8 only"; failed → cleared and
  `Hud_ShowMessage(0, 5 + bit5)` → type 6 "Last Spell Failed" for a last spell (5 "Spell Bonus Failed" otherwise).
- `Player_UpdateBomb` at the end of bomb index 4 (dissolve spell) clears bits 7–8, `Enemy_KillChildren(boss, 0)`,
  boss life 0, clears flag A bit 30, flash (3: 30, 1, 0xFFFFFFFF) (answers_impl11 §14).
- Side note: GameFlags bit 13 with a boss present makes `EnemyMgr_Calc` add a large `Player_DamageAgainst` box.

## 8. Explosion hit filter 0x450C50 — VERIFIED

- State 2 (already hit): return 1 (no damage this frame) if the bullet timer is odd, or a spell card is active and
  timer % 4 ≠ 0; otherwise damage /= 3 (min 1), velocity × 0.88.
- First hit: r = `Rng_FloatRange(π/2)` − 3π/4 (the RNG is drawn regardless of the sprite); sprite (+0x21A)
  0xC/0xE/0x10/0x12/0x14 → hitbox 48/64/80/96/128 and velocity = polar(r, 6).
- The type-5 effect (`Effect_Spawn(5, enemy pos, 1, white)` when bullet timer % 6 == 0) comes **after** both
  branches and only on non-filtered frames: effectively every 6th frame, every 12th during a spell card.

## 9. Alice laser damage areas and +0x3D — VERIFIED

`ShotUpd_AliceLaser` 0x450840: for i < n (+0x46A) with trail[2i].x (stride 0x18) ≥ −900:
`Player_AddDamageRect(trail point, 16 × 448, dmg 1, 0)` and the area's +0x3D := 1. This matches 14 §4.
`Player_AddDamageRect` 0x44DFA0 takes the first free of 0xBF slots at player +0xB8834 (stride 0x40), zeroes it
(+0x38 := 1), active +0x3C = 1, size +0x10/+0x14, +0x24 = arg 6, +0x2C = arg 5. The complete list of +0x3D
writers is in answers_impl11 §2.
