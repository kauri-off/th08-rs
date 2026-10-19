# Answers to docs/briefs/IMPL11_QUESTIONS.md (RE side)

Questions covered: **1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14** (and 15, which was already answered in
`answers_bugs1.md` §B2; not repeated here).

Sources:
- static reading of th08.exe: `Bomb_DrawTint` 0x40BC60, `Stage_RequestTint` 0x409160, `AnmMgr_TintOn/Off`
  0x40BAD0 / 0x40BAB0, `Stage_DrawLow` / `Stage_DrawHigh`, all bomb calc functions 0x40C010–0x413990,
  `Player_AddDamageCircle` 0x44E040 (+ slot reset 0x44E370), the effect updates/inits 0x40E040–0x40E2D0,
  0x410BB0, 0x4114E0, 0x411720, 0x4117B0, 0x411A80, `EffInit_Ring` 0x4272E0, `Effect_SpawnFixedParam` 0x4259E0,
  options 0x44EB70 / 0x44F930, `EnemyMgr_Calc` 0x42C660, `Enemy_Free`, `Gui_LoadFaces` 0x414590,
  `Gui_ShowPlayerSpellName` 0x415D60, `Gui_ShowBossSpellName` 0x415F00, 0x416130, `Spell_HideName` 0x416160,
  `Spell_Calc` 0x416B90, `Spell_Draw` 0x4178C0, 0x4664A0, `Player_UpdateBomb` 0x44C650, `Player_Die` 0x44AB40,
  `Spell_End` 0x4161B0, `GameManager_StageStart`;
- a capstone scan of every `[reg + 0x3D]` byte store in game code (new tool `re/tools/dispscan.py`);
- data: `etama.anm`, `face_cdbg.anm`, `text.anm`, `ascii.anm`, all `face_XX00.anm` / `face_*sp.anm`;
- runtime ref `specs/data/ref/spell54_cutin_f134.png` (phase 8; no new probe run).

Specs fixed in place: 06 §10.2 (tint lifetime, request averaging, sparkle bound), 14 §1.3, §2.1–2.4, §2.7–2.10,
§2.13, §2.14, §2.17, §3 table, §3.1; 12 new §5.8 (plates, cut-ins, bonus line); 13 §2.6 rows 36/37, 42–46; 09 §7
(dissolve end); 11 §2.1. `symbols.csv`: 0x40BAB0, 0x40BAD0, 0x409160, 0x416130, 0x4664A0, 0x44EB70, 0x44F930,
0x4117B0.

## 1. Bomb tint lifetime and flag 0x4EA49C — VERIFIED

Pieces:
- `Bomb_DrawTint` 0x40BC60 is the first thing every bomb draw function does (player draw, prio 9, after the
  flying player bullets): global tint **off** (0x40BAB0: AnmManager+4 := 0, colour 0x80808080); colour request
  via `Stage_RequestTint` 0x409160 (empty request (alpha byte +0x646B = 0) → store; else average each channel
  with the stored request); g_Stage+0x646C (= 0x4EA49C) := 1.
- `Stage_DrawLow` (prio 6): request present → tint **on** with +0x6468 (0x40BAD0); request reset to 0x00808080.
- `Stage_DrawHigh` (prio 7), at the end: +0x646C == 0 → tint off; then +0x646C := 0, +0x647C := 0.
- Others: the prio-11 enemy pass, `BulletMgr_Draw` (13) and `Spell_Draw` (15) turn the tint on (0xFFF01010) only
  while GameFlags bit 10 and turn it off afterwards; `Player_UpdateBomb` (bomb end) and `Player_DyingUpdate` turn
  it off in calc.

Consequence: +0x646C means "a bomb drew last frame". While a bomb runs, the tint switched on by frame N's
`Stage_DrawLow` (from frame N − 1's request) is **not** switched off by `Stage_DrawHigh`; it covers stage layers
0–3, background effects (list B), the spell-background VMs, the prio-8 enemy pass and the player's flying
bullets, until `Bomb_DrawTint` turns it off. Enemies of prio 11, effects (prio 12), bullets and the HUD are not
tinted. So the port's "off at the end of `Stage_DrawHigh`" is wrong while a bomb is active.
Edge frames: first bomb frame → no request yet → no tint; the frame after the last bomb draw → the pending
request still tints the stage, and +0x646C (set by that last bomb draw) keeps it on through prio 8/9 (the
calc-time tint-off at the bomb end runs before the draw, so `Stage_DrawLow` re-enables it); the next frame is
clean.

## 2. +0x3D ("no hit effect") per area — VERIFIED

Complete list of `mov byte [reg+0x3D], 1` in game code: `Bomb_Reimu` 0x40C1C1, `Bomb_ReimuLS` 0x40CAC1,
`Bomb_Marisa` 0x40E5BA / 0x40E5E9, `Bomb_MarisaLS` 0x40EAFA / 0x40EB29, `ShotUpd_AliceLaser` 0x450983.
`Player_AddDamageCircle/Rect` zero the slot (0x44E370: 16 dwords = 0, +0x38 := 1), so every other area has
+0x3D = 0.
- Reimu / Reimu LS: only the 16 D r 64 orb circles (dmg 5) have it. The impact circles (Reimu dmg 500, Reimu LS
  dmg 25) and the Reimu LS blasts (dmg 400) do **not** → they produce the normal hit spark (every 4th hit, 13 §2.6
  type 3).
- Marisa / Marisa LS: both damage rectangles.
- Alice laser: its one-frame rectangles.
answers_impl9 §1 was right per bomb but too broad per area; 14 §2.1/§2.2 now say it per area.

## 3. Reimu orb areas during the spin-out — VERIFIED

They follow the orbs during T < 40 as well. The final loop of `Bomb_Reimu` runs every frame for every orb in
state 1, after the movement: C area (r 96) and D area (r 64) positions := orb position. In the spin-out the orb
position is computed first (centre + (sin a, cos a)·radius), so the areas are at the orb's position of the
same frame; the C circle never stays at the start position. (Reimu LS does the same inside its per-orb loop.)

## 4. Reimu LS start effect — VERIFIED

Yes: at T = 0 `Bomb_ReimuLS` calls `Effect_Spawn(12, p, 1, 0xFF4040FF)` right after `Bomb_Start`, exactly as
`Bomb_Reimu`. (Also confirmed: the blast object index +0xFF0 is stored as the constant 1, so the first blast
uses object 16 and all later ones object 17, as 14 §2.2 says.)

## 5. Sakuya knife hit test — VERIFIED (guarded, one-shot)

The whole area block is inside "if the knife's D-area pointer (+0x16E8) ≠ 0". On a hit (area total +0x30 > 0,
age < 120): VM script 0x17 (LS 0x15), effect 0 (0xFFFF80FF), both areas' active byte := 0 **and both pointers
:= 0**, SE 0x2B. At age ≥ 120: both areas off and both pointers cleared. So the hit branch runs at most once;
it does not restart script 0x17 every frame. (Same in Sakuya LS, including the k % 3 == 0 flash.)

## 6. Alice ring colours — VERIFIED

`Bomb_Alice` (T = 90) and `Bomb_AliceLS` (T = 120) spawn 0x2A, 0x2B, 0x2C at the doll with colour
**0xFFFFFFFF**; 0xFFFFFFFF is right for the RGB. But the alpha is animated by their script 83 (etama entry 2
script 7, sprite 224, normal blend): alpha 0 → 255 over 8 frames, held, from t = 50 → 0 over 30, `hide_stop`
at 80 (freed then). The 0x2D octagons use the same script with their spawn colours (14 §2.3/2.4).

## 7. Alice ring 0x2A before t = 40 — VERIFIED

Amplitude 0, a plain ring. `EffUpd_AliceWave` 0x40E040 writes the amplitude (+0x32C) only when t ≥ 40
(64·k); for t < 40 it keeps the value from `EffInit_Ring` 0x4272E0, which sets +0x32C = 0 (also +0x330 = 0,
+0x334 = 0 before the update sets 5 waves). The ring renderer's "+0x32C = 0 → plain ring" path is used: radius
256k, width 8, 64 segments.

## 8. Remilia rings — VERIFIED

Both bombs, every advanced frame with T % 10 == 0 from T = 60: `Effect_SpawnFixed(0x35, p, slot 4 + (n % 4))`;
32 segments, 4 waves; `AnmVm_InterpPos`(30 frames, mode 4, from **(0, 0, 0)** to (192, R·128, 0)) (one
`Rng_FloatRange(128)` draw); `AnmVm_InterpScale`(30, **mode 1**, (64, 0) → (64, 0)) for Remilia,
(64, 0) → (128, 0) for Remilia LS; `AnmVm_InterpAlpha`(30, mode 3, 255 → 0); `AnmVm_InterpColour`(30, mode 0,
0xFFFFFFFF → 0xFFFF0000); VM run once; n += 1; SE 0x11 @p. There is **no** odd-n script switch (always the type's
script 88), unlike Marisa LS.

## 9. Team Remilia bats and Yuyuko options — VERIFIED

`Option_RemiliaTeam` 0x44EB70: init script 0x18; anchor = p + (−30, −16) / (−10, −32) / (+10, −32) / (+30, −16)
(index 0–3), then fixed; start orbit angle **0, π, 0, π** (same as solo); option timer ≥ 12 → orbit += +π/120,
−π/90, +π/90, −π/120; pos = anchor + polar(orbit, 8). It spawns **no** effect 0x2F and never changes colour.

`Option_Yuyuko` 0x44F930: init script 0x15; anchor = p; orbit 0 (index 0) / −π (index 1). Each frame: target =
p − (32, 0) with orbit += π/60 (index 0), p + (32, 0) with orbit −= π/60 (index 1); anchor += (target −
anchor)·0.09; pos = anchor + polar(orbit, 6), z 0; `Effect_Spawn(0x2F, pos, 1, **0x80602050**)` every frame.

## 10. Sakuya LS window — VERIFIED

SE 6 is still played every frame of the window 20 ≤ T < 148 (after the knife loop, so not on frames where the
loop returns early because a knife is still active); only the shake moved to T = 0. Effect 20 is spawned at p
at T = 0, kept (+0x1724) and moved to p every frame while 0 ≤ T ≤ 60, exactly as in 2.7.

## 11. Effect lifetimes — VERIFIED

All the bomb-owned ring updates (`EffUpd_SakuyaRing`, `EffUpd_YukariSquare`, `EffUpd_YukariLSSquare`) always
return 1, so they never free the effect; `EffectMgr_Calc` frees it when the VM reaches `hide_stop`:
- 0x2E (Sakuya LS, script 84 = etama entry 2 script 8): alpha 255, from t = 8 → 0 over 7, `hide_stop` at
  **t = 23**; the update adds 8 to the radius every frame (12 segments, width 32).
- 0x24 (Yukari): `EffInit_YukariBoundary` re-spawns the same fixed slot as type 0x23 via
  `Effect_SpawnFixedParam`, which re-zeroes the slot and loads type 0x23's script **88**; the bomb then sets
  0x59 / 0x5A / 0x5B on slots 5–7. Scripts 88–91: additive, alpha 0 → 255 over 20, at 80 → 0 over 60, freed at
  **t = 140** (the shrinking square ends at t = 40; the side rectangles are separate areas with life 70 / 100).
- 0x25 (Yukari LS): the same re-spawn, so slot 4 runs script **88 (life 140)**, not 0x5C (QUIRK); slots 5–7 get
  0x5D / 0x5E / 0x5F: alpha 0 → 255 over 20, at 160 → 0 over 60, freed at **t = 220**.
A later spawn into the same fixed slot (next Yukari bomb, Marisa LS sparks, dissolve ring) replaces them earlier.

## 12. Yukari option aimed enemy — VERIFIED

The pointer at player +0xE2ABC (0x18B89B4) persists across frames and is never re-picked from scratch.
`EnemyMgr_Calc`, for every live enemy with flag A bits 4, 5 and 11 clear and (bit 31 clear or not bombing),
replaces it when |enemy final x − p.x| < 64, the enemy has no parent (+0x2DA4 = 0), and (pointer = 0 or enemy
final y (+0x2D8C) < aimed enemy's base y (+0x2D38)) — the comparison mixes the final and base position fields
(QUIRK). It is cleared only when the aimed enemy's slot is found inactive at the start of its update, by
`Enemy_Free`, or by `Option_Yukari` itself (sub-states 0–2 when not attacking, leaving sub-state 3). So in
sub-state 3 Yukari keeps chasing the same enemy after it leaves the 64-px column, unless a higher enemy enters
the column.

## 13. Spell-name plate and cut-ins — VERIFIED (new spec 12 §5.8)

Nothing in the specs described this. It is now in 12 §5.8. Summary:
- `g_Gui` owns 14 VMs. Player: cut-in (face slot 16/17 script 0), `face_cdbg` strips (scripts 0 + 2, sprite =
  isLastSpell), card name (`text.anm` script 4, left-aligned, RGB (255, 240, 240)), name plate (`ascii.anm`
  script 1, sprite 134, drawn at text pos − (32, 0)). Boss: cut-in (slot 18 script 0, sprite = face index), strips
  (cdbg scripts 1 + 3 into the **same** two VMs), name (`text.anm` 5, or 6 + glow copies 7/8 for ids 0xCD–0xDD,
  right-aligned, RGB (240, 240, 255)), plate (`ascii.anm` script 0, sprite 133, drawn at the text pos), label
  "Bonus … history" (`ascii.anm` script 2) with the bonus / captures / attempts digits.
- Player plate show: `Bomb_Start` → `Gui_ShowPlayerSpellName(EDX face, name, isLastSpell)`; hide: 0x416130 at the
  bomb end (text interrupt 1 → slides to (−128, 440), plate interrupt 2). Boss show: `Spell_Start` / native 22;
  hide: native 23 only.
- QUIRKs: the strips are drawn only inside the "player cut-in visible" block, so the boss strips never show
  unless a player cut-in is visible (consistent with `spell54_cutin_f134.png`); the dissolve spell passes face −1
  (no cut-in, so no strips); Youmu LS passes isLastSpell 0 (normal strip sprite); the computed plate widths
  (+0x2618 / +0x261C) are never read; VMs 3 and 5 are never given a script.

## 14. Dissolve spell trigger and end flash — VERIFIED

- The "special mode" is GameFlags (0x164D0B4) bits 7–8 ≠ 0: bit 7 = a boss **last spell** is running (set by ECL
  op 176 before the card; answers_impl10 §7), bit 8 = a last spell was captured (`Spell_End`), kept until the
  next op 176, the dissolve end, or `GameManager_StageStart` (which clears 7, 8 and 13). Bit 14 (spell practice)
  is separate: it blocks bomb presses but does not select the dissolve spell.
- While bits 7–8 ≠ 0 or bit 14 is set, a bomb press only plays SE 0x29 (on the press edge). The dissolve spell
  (index 4) starts only as the forced bomb of a hit: `Player_Die` in the special mode sets counter 2 and +6 = 1;
  `Player_UpdateBomb` then starts a bomb with index 4 because bits 7–8 ≠ 0. So in spell practice it happens only
  on cards whose script runs op 176 (40-frame variant, 14 §2.17).
- End flash: `ScreenEffect_Register` type 3, EDX p0 = **30**, repeats 1, colour **0xFFFFFFFF** (white, alpha 255
  → 0 over 30 frames). It comes after bits 7–8 are cleared and every boss gets `Enemy_KillChildren(boss, 0)`,
  life := 0 and flag A bit 30 cleared.

## 15. B2 death ring

Already answered and implemented (`answers_bugs1.md` §B2).
