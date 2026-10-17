# 13 — Sound effects, effect manager, screen effects, score popups

Sources: `Sound_LoadSE` 0x45D3F0, `Sound_LoadWav` 0x45C8C0, `Sound_Play` 0x45D550, `Sound_PlayAt` 0x45D660,
`Sound_Update` 0x45D790 (called from `Frame_Run` right after the calc chain); effect manager 0x4ECE60
(`EffectManager_Register` 0x428620, `Effect_Spawn` 0x425430 and variants, `EffectMgr_Calc` 0x427BF0, draw
0x427F00 / 0x428100 / 0x4281E0, ring renderer 0x427450); `ScreenEffect_Register` 0x45B8B0; popups in the
ASCII manager (0x403200 / 0x403330 / 0x403460 / 0x403600, `Ascii_Calc` 0x402200, `Ascii_DrawStrings`).
VERIFIED unless marked. Coordinates: playfield space (00 §5) unless "screen".

## 1. Sound effects

### 1.1 Files and the SE table (VERIFIED, tables 0x4C81B0 / 0x4C8040)
36 wave files (loaded from the archive by name, RIFF/WAVE parsed, one DirectSound buffer each, flags
0x80C8 = static | ctrl pan | ctrl volume | ctrl frequency-less | get-current-pos2 INFERRED), then 46 SE ids,
each a duplicate of a wave buffer: record = {i32 wave index, i16 volume, i16 priority}. Volume is in 1/100 dB
(DirectSound attenuation) and is applied once at load; the priority word is copied into the manager when the
id is queued but nothing reads it (INFERRED unused).

| id | hex | wave | vol | prio | | id | hex | wave | vol | prio |
|---|---|---|---|---|---|---|---|---|---|---|
| 0 | 0x00 | se_plst00 | −1900 | 0 | | 23 | 0x17 | se_tan01 | −1800 | 20 |
| 1 | 0x01 | se_plst00 | −2100 | 0 | | 24 | 0x18 | se_tan02 | −1800 | 20 |
| 2 | 0x02 | se_enep00 | −1200 | 5 | | 25 | 0x19 | se_kira00 | −1100 | 50 |
| 3 | 0x03 | se_enep00 | −1500 | 5 | | 26 | 0x1A | se_kira01 | −1300 | 50 |
| 4 | 0x04 | se_pldead00 | −1100 | 100 | | 27 | 0x1B | se_kira02 | −1500 | 50 |
| 5 | 0x05 | se_power0 | −700 | 100 | | 28 | 0x1C | se_extend | −500 | 140 |
| 6 | 0x06 | se_power1 | −700 | 100 | | 29 | 0x1D | se_timeout | −500 | 100 |
| 7 | 0x07 | se_tan00 | −1900 | 50 | | 30 | 0x1E | se_graze | −1100 | 20 |
| 8 | 0x08 | se_tan01 | −2200 | 50 | | 31 | 0x1F | se_powerup | −800 | 90 |
| 9 | 0x09 | se_tan02 | −2400 | 50 | | 32 | 0x20 | se_graze | −1200 | 20 |
| 10 | 0x0A | se_ok00 | −1100 | 100 | | 33 | 0x21 | se_kira00 | −500 | 50 |
| 11 | 0x0B | se_cancel00 | −1100 | 100 | | 34 | 0x22 | se_pause | −800 | 100 |
| 12 | 0x0C | se_select00 | −1500 | 10 | | 35 | 0x23 | se_cardget | −800 | 100 |
| 13 | 0x0D | se_gun00 | −1500 | 10 | | 36 | 0x24 | se_option | −800 | 100 |
| 14 | 0x0E | se_cat00 | −1000 | 100 | | 37 | 0x25 | se_damage01 | −700 | 0 |
| 15 | 0x0F | se_tan00 | −1100 | 50 | | 38 | 0x26 | se_timeout2 | −300 | 100 |
| 16 | 0x10 | se_lazer00 | −1300 | 50 | | 39 | 0x27 | se_opshow | −800 | 100 |
| 17 | 0x11 | se_lazer01 | −1400 | 50 | | 40 | 0x28 | se_ophide | −800 | 100 |
| 18 | 0x12 | se_enep01 | −900 | 100 | | 41 | 0x29 | se_invalid | −200 | 100 |
| 19 | 0x13 | se_nep00 | −400 | 100 | | 42 | 0x2A | se_slash | 0 | 100 |
| 20 | 0x14 | se_damage00 | −880 | 0 | | 43 | 0x2B | se_slash | −600 | 100 |
| 21 | 0x15 | se_item00 | −1500 | 0 | | 44 | 0x2C | se_item01 | −800 | 0 |
| 22 | 0x16 | se_tan00 | −300 | 20 | | 45 | 0x2D | se_ok00 | −100 | 100 |

All files are under the archive names shown with ".wav" (th08.dat, 01_archive). Tool: none needed (the table
is in the exe); the wave files are ordinary PCM RIFF files.

### 1.2 Queueing (`Sound_Play(id, pan)`, `Sound_PlayAt(id, x)`)
- `Sound_PlayAt` converts x to pan = trunc((x − 192) × 1000 / 192) (−1000 left … +1000 right at x = 384);
  `Sound_Play` takes the pan directly (always 0 at the call sites).
- The manager holds a per-frame queue of up to **12 distinct ids** (+0x620, terminated by −1) and for each queued
  id up to 128 pan values (+0x680 + slot × 0x200; count at +0x650).
- Queue rule: if the id is already queued this frame → its pan list gets one more value (ignored when it already
  has 128); else the id takes the next free slot; if all 12 slots are used the request is **dropped**.
  So every SE id sounds at most once per frame (per-frame dedup), with the average pan of all requests.

### 1.3 Flush (`Sound_Update`, once per frame after all calc callbacks, also while paused)
Skipped entirely when config "SE on" (cfg byte 0x17CE890, default 1) is 0 — the queue is then left as is
(INFERRED: it is not cleared). Otherwise for each queued slot in order: clear the slot; stop the id's buffer,
rewind to 0 (so a still-playing instance restarts), set pan = integer average of its pan list, set volume
(below), play once (non-looping).
Volume: v = cfg SE volume (byte 0x17CE898, 0–100, default 80). v = 0 → −10000 (silent). Else
k = 1 − (1 − v/100)³ and volume = trunc((tableVolume + 5000) × k) − 5000. (The table volume set at load is
overwritten by this every time.) The same curve with the BGM volume byte 0x17CE897 drives music (02).
The BGM command queue (+0x1F88) is processed in the same call (02_bgm).

### 1.4 Call sites (VERIFIED; `@x` = panned at that x)
| event | id | where |
|---|---|---|
| player shot | sht entry +0x26 (all shipped entries use 0 or −1) @player | `Player_InitShot` (09 §4.3) |
| graze (bullet or laser) | 0x1E @bullet | `Player_OnGraze` |
| player hit | 4 @player | `Player_Die` |
| real death (deathbomb window over) | 0xF @player | `Player_DyingUpdate` |
| bomb pressed while bombs are forbidden (special / spell-practice modes) | 0x29 | `Player_UpdateBomb` |
| bomb / last spell / boss spell card declared (name banner) | 0xE | `Gui_ShowPlayerSpellName` 0x415D60, `Gui_ShowBossSpellName` 0x415F00 |
| bomb choreography sounds | 5, 6, 0xD, 0xF, 0x11, 0x13, 0x19, 0x2A, 0x2B | per bomb (14 §2) |
| spell card captured | 0x23 | `Spell_End` 0x4161B0 |
| any spell card end (captured or not) | 0xF | `Spell_End` |
| enemy damaged (frame with damage > 0, not invincible) | 0x14 @enemy; 0x25 when the enemy's low-life warning level (flags B bits 4–5) ≥ 2 | `EnemyMgr_Calc` |
| enemy destroyed | 2 or 3 @enemy (by enemy slot parity: even 2, odd 3) | `EnemyMgr_Calc`, `Enemy_KillChildren` |
| enemy reacting to a player mode switch (flag A bit 8) | 0x28 to youkai, 0x27 to human | `Enemy_ReactPlayerMode` 0x42C420 |
| child enemy spawn (ops 90–92) | 0x24 @enemy | `Ecl_Run` |
| ECL op 124 | arg @enemy (data uses 5, 7, 8, 13, 15, 16, 17, 18, 19, 25, 33, 36, 42) | `Ecl_Run` |
| ECL op 181 (clock +1) | 0x2D | `Ecl_Run` |
| bullet pattern fired with descriptor flag 0x200 | descriptor +0x200 @origin x (default 7; op 113 sets it; data: 7, 15) | `BulletMgr_SpawnPattern` (08 §3) |
| bullet ex-command sound (descriptor +0x204 "ex sfx", default 25 (0x19)) on redirect / bounce / accel start | ex sfx, centre | `Bullet_Ex*` (08 §5) |
| ex command 0x80000 | c @bullet (data: 15, 16, 25, 27) | `Bullet_ApplyExCmds` |
| item collected | 0x15, or 0x2C if an item reached the maximum point value this frame — one per frame | `ItemMgr_Calc` (08 §8) |
| power level up / full power | 0x1F | 0x440CF0, 0x441170, `ItemMgr_Calc` |
| extend (life or bomb) | 0x1C | `GameMgr_Extend`, result 0x4550FC |
| pause opened | 0x22 | `GameMgr_Calc` |
| boss timer tick (value changed) | 0x26 when < 3 s, else 0x1D when < 10 s | `Hud_DrawBoss` (12 §5) |
| dialogue: card-get gift / choice | 0x23; choice cursor 0xC, confirm 0xA | `Msg_Run` (10) |
| menus: confirm / cancel / cursor | 0xA / 0xB / 0xC | main menu, option, replay, practice (12) |
| option menu value change | 0x1D | `MainMenu_Option` |
| locked spell-practice card | 0x29 | `MainMenu_SpellPracticeCard` |
| pause & continue menus: cursor / confirm | **0** (`se_plst00`) / 0xA | `PauseMenu_Update`, `ContinueMenu_Update` |

Ids with no reference found in code or data: 1, 9, 22, 23, 24, 26, 32 (INFERRED unused).

## 2. Effect manager (`g_EffectMgr` 0x4ECE60)

### 2.1 Layout (VERIFIED)
| off | meaning |
|---|---|
| +0 | ring cursor (0..511) |
| +8 | live count (recomputed every calc) |
| +0x1C | effects: 653 × 0x360 (index 0–511 ring pool, 512–639 "high" pool, 640–652 fixed slots 0–12) |
| +0x89BFC | dummy effect returned when a pool is full (index 653) |
| +0x89F5C … +0x8ACDC | five list-head pseudo-effects (one 0x360 stride each; head.next at head+0x35C) |
| +0x8B03C … +0x8B04C | five list tail pointers |
| +0x8B050 | frame counter (anti-tamper: every 300 frames (≡100) a checksum test may return 4 = quit; ignore) |
| +0x8B054 | ANM file slot 6 (`etama.anm`) — all effect scripts come from here |
| +0x8B058 | ANM file slot 9 (`effNN.anm`, spell-card backgrounds, §5) |

Effect (0x360):
| off | type | meaning |
|---|---|---|
| +0 | AnmVm | sprite (flags bit 13 "z-write off" set by the spawners) |
| +0x288 | f32[3] | VM pos offset (spawners zero it; some updates use it as an attractor point) |
| +0x2A4 | f32[3] | position |
| +0x2B0 | f32[3] | spawn parameter (`Effect_SpawnWithParam`) |
| +0x2BC | f32[3] | velocity |
| +0x2C8 | f32[3] | acceleration |
| +0x2D4 | f32[3] | world position (3D particles) |
| +0x2E0 | f32[3] | centre (orbit/ring effects) |
| +0x2EC | f32[3] | direction / axis |
| +0x314 … +0x334 | | ring parameters (§2.5) |
| +0x328 | i32 | fixed-slot index |
| +0x338 | Timer | age |
| +0x348 | fn | update (return 1 = keep, 0 = free) |
| +0x34C | fn | custom draw (ring renderer) |
| +0x350 | u8 | active |
| +0x351 | u8 | type |
| +0x352 / +0x353 | u8 | "fade-out" request / counter |
| +0x354 | u8 | layer: 0 normal 2D, 1 3D billboard (background), 2 3D (custom matrix), 4 2D in background pass |
| +0x355 | u8 | draw above bullets |
| +0x356 | u8 | ring geometry dirty |
| +0x357 | u8 | keeps running while bullets are frozen |
| +0x358 | ptr | heap vertex buffer (0x1C38 bytes; freed when the slot is reused / on remove) |
| +0x35C | ptr | draw-list link |

### 2.2 Spawning (all return the effect, or the dummy when no slot is free)
Common init: slot zeroed (heap buffer freed), active = 1, type, pos, VM script = table[type].script on slot-6
file, VM flag 0x2000 (z-write off), VM colour = the colour argument, pos offset = 0, update = table[type].update;
then table[type].init runs (ECX = effect); init returning ≠ 0 frees the slot immediately. Every spawn also sets
bit 0x400 of a HUD repaint word (INFERRED: unrelated bookkeeping).
| function | args | slot choice |
|---|---|---|
| `Effect_Spawn` 0x425430 | type, pos, count, colour | ring pool: advance cursor (wraps 512 → 0), take the first inactive slot; repeat for count effects (consecutive slots after the cursor); search at most 512 slots |
| `Effect_SpawnWithParam` 0x425650 | type, pos, param, count, colour | same, also param → +0x2B0 (VM flag 0x2000 not set) |
| `Effect_SpawnHigh` 0x425B70 | type, pos, count, colour | first inactive among 512–639 (no cursor); age timer 0; layer 0 |
| `Effect_SpawnFixed` 0x425870 | type, pos, slot k, colour | always slot 640 + k (overwrites); skips the script when table script < 0 |
| `Effect_SpawnFixedParam` 0x4259E0 | type, pos, param, slot k, colour | same with param |

Fixed slots in use: 0 invulnerability ring, 1 spell-card start ring, 2 youkai aura, 3 death burst (type 12), 11 deathbomb-window ring (type 59),
4 dissolve spell, 4–7 Yukari boundaries / Marisa sparks, 8 gauge-max aura, 9–10 boss natives, 12 stage.

### 2.3 Per frame (`EffectMgr_Calc`, calc prio 13)
For all 653 slots: inactive → free its heap buffer. Active: count it; if bullets are frozen (0x164D0B4 bit 10)
and +0x357 = 0 → not updated (still drawn). Else: update fn (if any) → 0 frees; then `AnmVm_Run` → script
finished frees; else age ticks. Survivors are linked (in slot order) into a draw list unless type = 0x40:
- layer 1 or ≥ 3 → list B (background); layer 2 → list C; layer 0: +0x355 → list D (over bullets),
  else VM blend additive → list E, else list A.

### 2.4 Draw
| list | drawn by | how |
|---|---|---|
| A (alpha) then C then E (additive) | `EffectMgr_Draw` 0x427F00, draw prio 12 (after enemies' 2nd layer, before bullets) | A/E: 2D rotated sprite at pos + playfield origin + pos offset, z 0.07, or the custom draw fn; C: billboard at pos (3D) |
| D | end of `BulletMgr_Draw` (prio 13, over bullets) | as A, z 0.04 |
| B | `Stage_DrawHigh` (after stage layers 2/3, stage camera, fog already off) | only at cfg effect level (byte 0x17CE894) 2: levels 0 **and 1** draw nothing (the parity test returns on the first element, asm 0x428224); layer 4 → 2D rotated, layer 1 → billboard (types 0x33/0x3F through the screen-space boss-pull / player-push helper 0x428310, answers_impl9 §8), else 3D quad |

### 2.5 Ring renderer (custom draw 0x427450; ring types, §2.6)
The ring is a triangle strip of 2(n+1) pre-transformed vertices (FVF XYZRHW|DIFFUSE|TEX1, colour = VM colour)
rebuilt whenever +0x356 is set (the ring updates set it each frame). **Texture: v runs along the ring** from
sprite.v1 to sprite.v0 (normalised, + uv_off_v; k-th pair v = v1 − k·(v1−v0)/n), **u across**: outer edge u0,
inner edge u1 (+ uv_off_u); WRAP addressing, so a 16×768 sprite in a 128-px texture repeats 6× around the
ring (`Anm_RingSetUV` 0x464B00; answers_impl9 §P2). Geometry:
n = segments (+0x324), r = radius (+0x314, from VM pos.x), w = half width = (+0x320, VM scale.x) /
sin((π − 2π/n)/2), start angle +0x318 (VM rotation z), step 2π/n, centre +0x2E0 (+ playfield origin).
- +0x32C (VM pos.y) = 0 → plain ring: outer r + w, inner r − w.
- else if +0x334 = 0 → rotated **ellipse**: vertex = rotate((cos θ·(r ± w), sin θ·(a ± w)), +0x330) with
  a = +0x32C, θ = 0, 2π/n, … (used by effects 0x2B/0x2C and the Youmu slash 0x30, 14 §2).
- else → wavy ring: radius offset cos(φ)·(+0x32C), φ starts at +0x330 (VM rotation y), step 2π·(+0x334)/n.
Ring effects copy VM fields into these parameters every frame, so their ANM scripts animate the ring.

### 2.6 Effect types (table 0x4C6D30: script, update, init; scripts are global `etama.anm` indices,
see `extracted/anm_dis.txt` — NOTE: anm_dis numbers scripts per entry; global index = entry-0 count (25) + entry-1 index for entry 1, + 51 more for entry 2 (e.g. 44 = entry 1 script 19, 78 = entry 2 script 2); "life" = frame at which the script stops, ∞ = loops until freed)
| type | script (life) | init / update behaviour | used by |
|---|---|---|---|
| 0 | 28 (20) | — | item conversion, enemy death burst (default death-effect byte 0), Sakuya knives |
| 1, 2 | 29 (20), 30 (40) | — | — |
| 3 | 31 (40) | spark (additive, scale 3 → 0): v = ((R·256 − 128)·4/33, same for y) (**RNG** x then y), a = −v/20; per frame pos += v, v += a (both × gs at init) | **bomb/last-spell damage area** hits an enemy: every 4th hit (player u8 counter +0xE2A94 & 3 == 0, areas with +0x3D = 0 only), at the enemy box, colour 0xFFFFFFFF — never from bullets (answers_impl9 §P1) |
| 4–11 | 36–43 (30) | spark: v = (R·256 − 128)/12 per axis (**RNG**), a = −v/19; same motion | enemy death (type e+0x3311 + 4, ×4), drops (×3 / ×6), player death (type 6 ×16), bomb orbs |
| 12 | 44 (40) | — | death burst at the **real** death (fixed slot 3, `Player_DyingUpdate` when the deathbomb window reaches 0); also bomb starts (pool, 3 bomb fns). Colour 0xFF4040FF. Script 44 (= etama entry 1 script 19): sprite 214 (63×63, etama2.png), additive, alpha 255 → 0 over 40 frames, scale 0.5 growing +0.3/frame, `hide_stop` at 40 → the effect manager frees it (runtime: freed on age 40). Nothing else frees it |
| 13–15 | 45 (∞) | layer 2 (init zeroes axis/angle); update: pos = centre (+0x2E0) + offset rotated about the normalised axis (+0x2EC) by angle (+0x318) (quaternion; INFERRED geometry), z = 0; when +0x352 is set: +0x353 counts up, alpha = 255·(1 − n/16)… freed after 15 frames, scale grows | ECL op 128 (enemy-attached; the enemy writes axis/angle/centre every frame, 07 op 128) |
| 16 | 0 | — | — |
| 17, 27 | 32 (60), 34 (120) | converge: centre = pos, dir = unit at random angle (**RNG**); pos = centre + dir·(256 − 256·t/60) | ECL op 139 (charge-up) |
| 18 | 33 (240) | as 17 with 240 frames, keeps z | — |
| 19 | 51 (300) | 3D background petal/leaf: spawned at camera + facing/2 + random (**RNG**: 10 draws, answers_impl9 §4), drifts with velocity + accel, z-rotation spin; freed when outside the view cone (dot(normalised dir from camera, facing) < 0.94) or when z ≥ 0 (kept only while z < 0) | — |
| 20 | 56 | — | — |
| 21, 26 | 52 (90), 53 (90) | burst: angle = param.x if > 0 else random (**RNG**), dir × (1 + R·1.5) (**RNG**); pos = centre + dir·128·(1 − (1 − t/90)²) | — |
| 22 | 54 (50) | follows the player while its script runs | youkai aura (fixed slot 2, created on focus) |
| 23, 24 | 104 (I0 + 1; script sets I0 = 32, the spawner overwrites it) | no-op update (keep) | invulnerability ring (fixed slot 0, `Bomb_Start` → 0x40BF00): I0 := invulnerability frames, scale interpolates (slot 4, linear) from current to 1/16 over the invulnerability time, z spin × const, colour R 0xFF G 0x40 B 0x40, fades in to alpha 160 over 30 frames |
| 25 | 35 (∞) | — | gauge-max aura (fixed slot 8) |
| 28, 29 | 57, 58 (10) | — | focus-switch flashes (human 0x808080FF / youkai 0x80FF8080) |
| 30, 31 | 59, 60 (10) | — | enemy mode reaction (→ human 0x80803030 / → youkai 0x80303080) |
| 32–34 | 48–50 (10, interrupts) | — | enemy auras (`Effect_SpawnHigh` 0x20 + n, ECL op 174 / child ops) |
| 35, 39, 49, 52, 59 | 88, 76, 86, 77, 78 | ring (init 0x4272E0: 3 segments ×2 vertices buffer, custom draw; update 0x427990: radius := VM scale.x→width, VM pos.x→radius) | 39 spell-card start (slot 1), 52 spell start in special mode, 59 player death ring (slot 11, colour 0xFFF0404F), 49 Reimu LS |
| 36, 37 | 88, 92 | ring, bomb-owned update/init (Yukari 0x4114E0 / 0x411720 / 0x411A80) | Yukari boundaries |
| 40, 55 | 81, 87 | ring, update 0x4279D0 (segments := I0, waves := I1, width := scale.x, radius := pos.x, amplitude := pos.y, angle := rot z, phase := rot y) | Alice bomb (40), Reimu LS (55) |
| 41 | 82 | ring, update 0x40EB50 (48 segments, width 32, radius 64 / 72 alternating per frame) | no spawn site found (INFERRED unused) |
| 42–46, 48 | 83, 83, 83, 83, 84, 85 | rings with bomb-owned updates (0x40E040–0x40E2D0 Alice, 0x410BB0 Sakuya LS, 0x413070 Youmu) | 14 §2 |
| 50 | 80 | ring with 32 segments, ends after 120 frames | dissolve spell (slot 4) |
| 51, 63 | 73, 74 | 3D background particles (RNG 8 draws, answers_impl9 §4; view-cone cull only); when the boss (0xF54CC0[0]) has flag bit 3 the pos offset tracks the boss position 10%/frame, consumed by the screen-space draw helper (answers_impl9 §8); type 51 also tints colour2 by the stage overlay colour | ECL op 139 (stage-specific) |
| 53, 54 | 88 | ring update 0x427AE0: frees itself when VM alpha becomes 0 | Marisa sparks (slots 4+) |
| 56, 58, 65 | 96, 100, 98 | ring (init 0x427970 = ring + over-bullets), update 0x4279D0 | boss natives 3 / 6 / 20 (slots 9/10) |
| 57, 62 | 55, 75 | — | stage (0x3E sparkles, `Stage_Calc`) |
| 60, 61 | 102, 103 (110) | init: pos += polar(param.x, 256); rotation z = param.x + π/2 | ECL op 140 (directional) |
| 64 (0x40) | 77 | ring-like params copied from the VM each frame; centre = VM pos; never put in a draw list | stage (fixed slot 12, `Stage_Calc`) |
| 47 | 72 | — | option sparkles (Remilia/Youmu/Yuyuko options, colours 0x80602050 / 0x80405080) |
| 38 | 71 | — | enemy mode-reaction extra (flag B bit 1 + condition) |

R = `Rng_Float` from the game RNG (consumes replay-relevant RNG: every type with an RNG init draws during
gameplay). Exact per-type RNG draw counts: types 3–11 two draws, 17/18/27 one, 21/26 two (one if param > 0),
19 ten, 51/63 eight.

### 2.7 Main spawn sites (VERIFIED)
| event | effect |
|---|---|
| player bullet hits enemy | type 5 at the bullet when it enters its hit animation (09 §4.5); hit filters 0x450C50 (timer % 6) and 0x450EE0 (counter & 7) spawn type 5 too. No type 3 |
| bomb damage area hits enemy | type 3 every 4th hit (§2.6) |
| graze | type 8 at the bullet/player midpoint |
| enemy dies | death-effect type (e+0x3310, default 0) once + type e+0x3311 + 4 (default 4) ×4 at the enemy; scripted "survive" (death mode 3): death type ×3 |
| enemy drops items | type e+0x3311 + 4 ×3 (×6 on certain drops) |
| player hit | type 6 ×16 at the player (script 38: 30 frames, then freed); ring type 59 (slot 11) **only when bombs ≥ 1 and not in dissolve mode** (game flags bits 7–8 = 0). The ring never ends by itself (script 78 loops, update 0x427990 always returns 1). It is freed by game code only: active := 0 and player+0xE2B28 := 0, either (a) by `Player_DyingUpdate` on the frame the deathbomb window reaches 0, just before the real-death effects, or (b) by `Player_UpdateBomb` when a (death)bomb starts. At real death: type 6 ×16 again and burst 12 (slot 3, 40 frames). VERIFIED (runtime, answers_bugs1 B2) |
| bomb start | invulnerability ring (type 23 slot 0) + per-bomb effects (14) |
| focus switch (teams, state held > 3 frames) | type 28 / 29; youkai side also creates the aura type 22 (slot 2) |
| gauge at human-max or youkai-max | aura type 25 (slot 8) |
| spell card start | ring type 39 (52 in special mode) at the boss: segments 64, plain ring (update 0x427990 copies only width = VM scale.x, script 192 → 15 / 20 over 120 frames, and radius = VM pos.x, 8 → 256 over 100 frames, ease mode 6; the +0x334 = 6 and pos.y values are unused); on capture `Spell_End` recolours it (alpha → 0x20) |
| player hit (death ring) | type 59 slot 11, colour 0xFFF0404F, 64 segments, radius VM pos.x 128 → 8 over the deathbomb window, ease mode 4; script 78 (sprite 226, one texture repeat) |
| ECL | op 128 → type 13 (colour 0xFF6060D0); op 139 → effect id × n (data: 17, 40, 51, 63); op 140 → `Effect_SpawnWithParam` (data: 26, 60, 61); op 174 / children → 0x20 + n via `Effect_SpawnHigh` |
| item conversion to stars | type 0 at the item |

## 3. Screen effects (`ScreenEffect_Register` 0x45B8B0, object "ScreenInf" 0x34 bytes, calc prio 3)
Call: ECX = type, EDX = p0, stack: p1 (+0x18), p2 (+0x1C), p3 (+0x20), draw priority. Object: +0 type,
+0x10 current alpha (or amplitude), +0x14 p0, +0x18 p1, +0x1C p2, +0x20 p3, +0x24 release flag, +0x28 Timer.
Calc returning 0 removes the effect (on-remove frees it).
| type | p0 / p1 / p2 / p3 | behaviour | draw |
|---|---|---|---|
| 0 | dur / colour RGB | fade **from** colour: alpha = trunc(255 − 255·t/dur); ends t ≥ dur | full screen 640×480 (resets the viewport) |
| 1 | dur / amp start / amp end | **shake**: a = p1 + t·(p2 − p1)/dur; screen offset x and y each = one of {0, +a, −a} chosen by `Rng_Range(3)` (**2 RNG draws/frame**, x first); frozen (no tick, no RNG) while bullets are frozen or the freeze byte 0x160F534 is set or 0x18B8A2C ≠ 0; ends t ≥ dur (offset left as is, but the popup draw 0x405420 at draw prio 14 zeroes AnmManager +0x1C/+0x20 every frame, so draws after prio 14 — HUD, ASCII strings — never shake) | none |
| 2 | dur / colour | fade **to** colour: alpha = trunc(255·t/dur); timer holds while paused/continue menu; ends at dur; returns 0 when 0x18B8A2C ≠ 0 | playfield (32,16)–(416,464) |
| 3 | dur / repeats / ARGB | **flash**: alpha = A − trunc(t·A/dur) (A = colour alpha); at dur: repeats −= 1, restart; ends at 0 repeats | playfield, colour = p2 RGB |
| 4 | dur / colour | as 2 (fade to colour) | full screen |
| 5, 6 | dur / colour | darken: alpha = trunc(128·t/dur) up to dur then held; when +0x24 is set: alpha = 128 − 128·t/8, ends after 8 frames | 5 full screen, 6 playfield |
| 7 | amp / ramp-in / hold / ramp-out | **shake** envelope: a = amp × (t/in, 1, (in+hold+out − t)/out); offsets as type 1 (2 RNG/frame); only when frames-since-stage-start (0x164D2C8) ≥ 2, frozen like type 1; ends after in+hold+out | none |

Used (type: p0, p1, p2): bomb hits / impacts 1: 16, 8, 0 and 3: 8, 1, colour; Marisa sparks 7: 16, 120, 60, 120;
Sakuya 1: 120, 4 and 1: 50, 4; Yuyuko 1: 120, 12 / 60, 16 / 30, 8; boss natives (below); `Player_UpdateBomb`
(dissolve-spell end) 3: 30, 1, colour; dialogue op 14: type 4, 442 frames (10 §5); demo end type 2; title
type 0, 70 frames.
Correction applied to 07 §12 (the lists there omitted the EDX argument p0): native 1 is
(3, p0 = 60, 1, 0xFFFFFFFF), native 10 is (3, 30, 5, 0x40FFFFFF) + (7, 4, 120, 190, 60), native 15 is
(7, 16, 20, 20, 20), native 17 is (3, 180, 1, …).

## 4. Score popups (ASCII manager)
Small popups: 720 + 3 slots × 0x38 at Ascii +0xB4DC: digits[≤12] (least significant first; glyph 10 = special
sign used for negative arguments), +0xC pos (+ playfield origin at spawn, i.e. screen coords), +0x18 colour, +0x1C Timer, +0x30 active,
+0x31 digit count. `Popup_Number` 0x403200(pos, value, colour) uses slots 0–719 round robin; 0x403330 uses
the 3 slots 720–722 (round robin).
Per frame (`Ascii_Calc`, not while paused/continue, not while bullets frozen): y −= 0.5·gs; age ticks; freed at
age ≥ 60. Draw: text centred (x − 4·digits, 8 px per glyph, ascii.anm sprites: digit glyph g → sprite g for
age < 52, g + 11 for age 52–55, g + 21 from 56), alpha by squared distance d to the player: d ≤ 1024 → 0x50,
1024 < d ≤ 4096 → 0x50 + (d − 1024)·128/3072, else 0xD0 (popups near the player are faded).
Glyph 10 (negative value) is sprite 10 (48×8) drawn left-anchored at x − 4 (Ascii VM anchor 0x1800), fading
through sprites 21 / 31 (faithful quirk). Colours per caller: answers_impl9 §12.
Large popups: 128 slots at +0x15304 (`Popup_Large` 0x403460 / `Popup_LargeStatic` 0x403600; glyphs stored
LSB first: [0xF, p4 digits, 0xE,] p3 digits, 0xD; ascii glyphs 0xD '+', 0xE '(', 0xF ')' → text "+p3" or
"+p3(p4)"; scale from the manager; sprite = glyph + 0x88), life 90 frames (0x403600 starts at age 88 = a
two-frame display). Callers: `Enemy_KillChildren` (chain kills, boss chain, single) and `Hud_DrawBoss`
(answers_impl9 §14). Small-popup callers: items (08 §8.5), cancel bonuses (08 §6), kill_all.

## 5. Spell-card background
`Spell_Start` calls `Stage_StartSpellBg` 0x415CE0 (stage +0xB24 = 1, +0xB28 = 0) and sets the stage's spell
VMs (stage +0xB38 + i × 0x2A4, i < count +0xB30) to scripts +0xB34 + i of the slot-9 file. The effect manager
on-register loads slot 9: `effNN.anm` by stage index (table 0x4C7480), or in spell practice for card ids ≥ 0xD8
the per-character `eff09xx.anm` (table 0x4C7144); it sets count = 2, first script = 0 (ECL native 5 resets the
count to 2). Each `Stage_Calc`: if +0xB24 > 0 → when +0xB28 == 60, +0xB24 = 2 (stage background layers no
longer drawn, 06 §7); +0xB28 += 1; run the spell VMs. `Spell_End` (0x416AD0) sets +0xB24 = 0.

## 6. Open points
- (closed) Large popup glyphs 0xD/0xE/0xF = '+', '(', ')' — VERIFIED (answers_phase6, answers_impl9 §14).
