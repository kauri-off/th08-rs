# 09 — Player

Sources: `Player_Register` 0x44C230, `Player_Calc` 0x44C390, `Player_Load` 0x44D650, `Sht_Load` 0x44DD70,
`Player_Move` 0x44AEC0, `Player_UpdateBomb` 0x44C650, `Player_Die` 0x44AB40, `Player_DyingUpdate` 0x44CBF0,
`Player_RespawnUpdate` 0x44D180, `Player_UpdateInvuln` 0x44D2C0, shooting 0x451500 / 0x450F60 / 0x44FB70,
player bullets 0x451150, damage 0x451670, collision tests (08 §4, 0x449FF0–0x44A930), bombs (table 0x4C7AD0).
Tool: `tools/shtdump.py` (all 8 `.sht` files parse with 0 problems; dump `extracted/sht_dump.txt`).
VERIFIED unless marked. Coordinates: playfield space (08 §1). gs = game speed.

## 1. Object, registration, per-frame order

Player object: global 0x17D5EF8, 0xE2B30 bytes, zeroed on registration (the two sht pointers survive when the
manager is re-registered without reloading). Calc priority 9 (runs before enemies/bullets); draw priorities 9
(0x44D530) and 10 (0x44D630).

`Player_Calc` per frame:
1. If the global freeze byte 0x160F534 is set (ECL native 26; used by ecldata_sk.ecl sub1): VM flag 0x80000 is set on the
   youkai aura effect (+0xBE834) and the gauge-max aura effect (+0xE2B24), each if present; return at once (no areas,
   bomb, death/respawn, invulnerability, movement, VM, bullets, shot timer, gauge fade, counters). Otherwise the flag is
   cleared on both and the calc goes on (VERIFIED 0x44C390, RE-Q24).
2. Damage/cancel areas update (§8.1).
3. `Player_UpdateBomb` (§7).
4. State 2 (dying): `Player_DyingUpdate`; if it returns 1 (respawn starts) → `Player_RespawnUpdate` runs too.
   State 1 (respawning): `Player_RespawnUpdate`.
5. `Player_UpdateInvuln` (§6.4).
6. If state is not 1/2: `Player_Move` (§3; also options §5, shot trigger, gauge §9.3).
7. Run the player sprite VM (+0x10).
8. Player bullets update (0x451150, §4.4), shot timer / firing (0x451500, §4.2).
9. 0x44D420: nearest-target bookkeeping reset (+0xE2AA4/+0xE2AB0 = (−999, −999, 0), +0xE2AC0 = 0) and the gauge-readout
   fade by player position (`Player_UpdateGaugeFade`; full rule in 12 §5.9, VERIFIED).
10. Unless a dialogue is running: frame counters 0x164D318 (game) / 0x164D31C (stage) += 1; at human-max
    gauge: 0x164D324 / 0x164D32C += 1 and AddScore(100); else at youkai-max gauge: 0x164D320 / 0x164D328 += 1
    and AddScore(100) (VERIFIED; shown as the "over-80%" lines of the clear panel, 12 §5.6).

### 1.1 Field map (main fields)
| off | type | name | meaning |
|---|---|---|---|
| +0 | u8 | state | 0 normal, 1 respawning, 2 dying (deathbomb window), 3 invulnerable |
| +2 | u8 | | 1 after load |
| +3 | u8 | focus | 0 unfocused / human side, 1 focused / youkai side (2 right after load) |
| +4 | u8 | last-spell request | set while in the deathbomb window (§6.2) |
| +5 | u8 | **mode** | active character: copy of +3 once the focus state has been held > 6 frames; solos: shot type parity (even = 0 human, odd = 1 youkai). Read by enemies (07 §10.1 "player mode", op 90–92 child setup). NOTE: the enemy-death gauge ±200 (07 §10.3), graze/item code read **+3**, not +5 |
| +6 | u8 | forced bomb | set on death in the special mode (§6.1) |
| +8 | i32 | focus age | frames since the last focus change |
| +0xC | ptr | player ANM file (slot 5) | |
| +0x10 | AnmVm | player sprite | +0x28/+0x2C (VM scale y/x as written by death/respawn), +0x200 colour |
| +0x2B4 | f32[3] | pos | |
| +0x2CC | f32[3]×16 | trail | previous positions (shifted only on frames with movement) |
| +0x38C..+0x3A0 | f32[3]×2 | hitbox min/max | pos ∓ +0x3D4 |
| +0x3A4..+0x3B8 | f32[3]×2 | graze box min/max | pos ∓ +0x3E0 |
| +0x3BC..+0x3D0 | f32[3]×2 | item box min/max | pos ∓ +0x3EC |
| +0x3D4 | f32[3] | hitbox half size | (sht hitbox/2, same, 5) |
| +0x3E0 | f32[3] | graze half size | (sht graze/2, same, 5) |
| +0x3EC | f32[3] | item box half size | (sht collect/2, same, 5) |
| +0x3F8 | f32[2] | last displacement | velocity × gs |
| +0x404 / +0x408 | f32 | x / y speed multiplier | 1.0; bombs change it |
| +0x40C | 4 × 0x2F4 | options | §5 |
| +0xFDC | i32 | bombing | |
| +0xFE0 | i32 | bomb index | 0–4 (§7) |
| +0xFE4 | i32 | bomb duration | |
| +0xFEC | i32 | bombs used by the last spell | |
| +0xFF4 | Timer | bomb timer | |
| +0x1000 / +0x1014 | fn[5] ×2 | bomb calc / draw functions | from table 0x4C7AD0 |
| +0x1028 | 128 × 0x16F0 | bomb objects | per-bomb scratch |
| +0xB8828 | f32[3] | bomb origin | |
| +0xB8834 | 192 × 0x40 | damage areas | §8 |
| +0xBB834 | 192 × 0x40 | cancel areas | §8 |
| +0xBE834 | ptr | youkai aura effect | |
| +0xBE838 | 128 × 0x484 | player bullets | §4.3 |
| +0xE2A38 | 3 × 0x10 | laser slots | {Timer, bullet ptr} for laser-type shots |
| +0xE2A68 | i32 | deathbomb counter | §6 |
| +0xE2A6C | i32 | bomb cooldown | := 16 by `Spell_End` (07 §11.1, absolute 0x18B8964) — its only writer; decremented while ≠ 0 at the top of `Player_UpdateBomb` (once per `Player_Calc`; skipped on a forced-start call: +6 set and +0xE2A68 == 1 jumps straight to the bomb start, past the decrement, the bombing branch and the start tests — re-VERIFIED RE-Q30); a normal bomb start needs 0 (VERIFIED 0x4161B0, 0x44C650, RE-Q24) |
| +0xE2A70 | i32 | respawn clear frames | §6.3 |
| +0xE2A74 / +0xE2A78 | ptr | sht human / sht youkai | (also globals 0x18B896C / 0x18B8970) |
| +0xE2A7C | i32 | bomb active flag | := 1 at bomb start; := 0 by every non-bombing `Player_UpdateBomb` call whose normal start test fails (no press, tampered, dialogue, counter 0, bombs < 1, cooldown ≠ 0), so it stays 1 through the bomb and its end frame and drops on the next non-bombing call. A press that passes those
tests in the special mode (GameFlags bits 7–8 or bit 14; only sound 0x29, §7) returns before both writes: +0xE2A7C
is **kept** (VERIFIED 0x44C650, RE-Q30). Readers (absolute 0x18B8974): `ItemMgr_Calc` 0x440500 and `Item_CollectTimeOrb` 0x4412B0 (VERIFIED, RE-Q24) |
| +0xE2A80 | ptr[4] | laser entry per slot | |
| +0xE2A90 | i32 | cancel item type | set by bomb-area hits (08 §4.1), reset to 6 by every test |
| +0xE2A94 | u8 | effect counter | |
| +0xE2A98 | i32 | direction code | §3 |
| +0xE2A9C / +0xE2AA0 | f32 | last vx / vy (before gs) | |
| +0xE2AA4 / +0xE2AB0 | f32[3] | homing targets | −999 when none; +0xE2AA4 = boss closest in x, else lowest damageable enemy (filled by `EnemyMgr_Calc`); +0xE2AB0 is never written (always −999) |
| +0xE2ABC | ptr | aimed enemy | highest (smallest y) non-child damageable enemy within 64 px horizontally |
| +0xE2AC4 | Timer | shot timer | −1 idle, 0..19 firing cycle |
| +0xE2AD0 / +0xE2AE8 | Timer | gauge timers | §9.3 |
| +0xE2AF4 | Timer | state timer | death/respawn/invulnerability |
| +0xE2B0C | f32 | option base angle | −π/2 at load |
| +0xE2B1C / +0xE2B24 / +0xE2B28 | ptr | effects (invulnerability ring, gauge-max aura, deathbomb ring) | |
| +0xE2B2C | i32 | time-orb damage step | 40 (27 for solo human); also global 0x57AD30 |

## 2. Shot types and files
Shot type = global byte 0x164D0B1 (04 spec): 0 Reimu & Yukari, 1 Marisa & Alice, 2 Sakuya & Remilia,
3 Youmu & Yuyuko, 4 Reimu, 5 Yukari, 6 Marisa, 7 Alice, 8 Sakuya, 9 Remilia, 10 Youmu, 11 Yuyuko.
Game-manager byte +0x3DBA9 holds the same value: "solo human" = ≥ 4 and even (0x42F230), "solo youkai" = ≥ 4
and odd (0x42F270).

| shot | ANM (0x4C7CB0) | human sht (0x4C7CE0) | youkai sht (0x4C7D10) |
|---|---|---|---|
| 0–3 (teams) | playerNN.anm | plyNNa.sht | plyNNas.sht |
| 4/5, 6/7, 8/9, 10/11 | player00/01/02/03.anm | ply0Na.sht (human) / ply0Nas.sht (youkai) | same file as human |

Initial sprite script: 0 (human) or 5 (solo youkai).

### 2.1 `.sht` format (VERIFIED, `tools/shtdump.py`)
Header (0x38 bytes) + level table:

| off | type | meaning | values (a / as files) |
|---|---|---|---|
| +0 | u16 | 0 | |
| +2 | u16 | number of levels n | 6 (ply02as: 8) |
| +4 | f32 | bombs per life (set on respawn, trunc) | 3 |
| +8 | i32 | deathbomb base / bomb-allowed counter (§6) | 18 (Reimu/Yukari), 10, 10, 12 |
| +0xC | f32 | hitbox size | 1.65 / 2 |
| +0x10 | f32 | graze box size | 2.8 / 6 (Sakuya, Remilia) / 3 (Youmu, Yuyuko) |
| +0x14 | f32 | item attraction speed | 10 / 12 |
| +0x18 | f32 | item collection box size | 24 / 32 |
| +0x1C | f32 | point-of-collection line (y) | 128 / 160 |
| +0x20 | i32 | unknown (not referenced by the code found) | 8000 / 3000 |
| +0x24 | f32 | unfocused speed (orthogonal) | 4 / 5 |
| +0x28 | f32 | focused speed | 2, 2.2, 2.3, 1.9 |
| +0x2C | f32 | unfocused diagonal component | speed/√2 |
| +0x30 | f32 | focused diagonal component | |
| +0x34 | f32 | item fall multiplier (08 §8.4) | 0.9 (ply02a: 0.65) |
| +0x38 | {u32 off, i32 thr}[n] | levels | offset (file-relative, made absolute at load) and power threshold |

Which header is used: hitbox/graze/collect sizes, POC line, attraction speed, deathbomb base and bombs per life
always come from the **human** sht (+0xE2A74); movement uses the human sht's +0x24/+0x2C when unfocused and the
youkai sht's +0x28/+0x30 when focused; the item fall multiplier comes from the youkai sht when player +3 ≠ 0.

Level selection: the first level whose threshold > trunc(power) (thresholds 8, 24, 48, 80, 128, 999).
Exception: shot types 2 (with a youkai bomb, index odd) and 9, when the bomb timer ≥ 60: level 6 (bomb) or
7 (bomb index & 2 = last spell) — only ply02as has 8 levels (both threshold 999).

Shot entry (0x38 bytes; list ends with i32 −1):

| off | type | meaning |
|---|---|---|
| +0x00 | i16 | interval |
| +0x02 | i16 | phase: fires when shot timer % interval == phase (laser entries: laser slot) |
| +0x04 / +0x08 | f32 | spawn offset x / y |
| +0x0C / +0x10 | f32 | hitbox width / height |
| +0x14 | f32 | angle |
| +0x18 | f32 | speed |
| +0x1C | i16 | damage |
| +0x1E | i16 | side marker: > 0 youkai-side shot (drawn red at youkai max), < 0 human-side shot (time-orb drops at human max) |
| +0x20 | i16 | origin: 0 player, k = option k−1 position |
| +0x22 | i16 | bullet type: 3 piercing, 4/5 laser (not culled off-screen, hit every 2nd frame), 6 (no hit animation) |
| +0x24 | i16 | ANM script − 10 (bullet uses script +10, hit animation +11) |
| +0x26 | i16 | sound id (−1 none), panned at player x |
| +0x28 | i32 | init function index (table 0x4C7EE0) |
| +0x2C | i32 | update function index (0x4C7F04) |
| +0x30 | i32 | extra draw function index (0x4C7F1C) |
| +0x34 | i32 | hit filter function index (0x4C7F24) |

Function indexes (all values used by the 8 files are in range):

| table | idx | addr | behaviour |
|---|---|---|---|
| init | 0 | 0x44FD80 | default: fire on phase match |
| | 1 | 0x450240 | default, then if an aimed enemy exists (+0xE2ABC): angle = atan2 to its position (+0x2D34) + entry angle + π/2; vel = polar(angle, speed × 1.5) |
| | 2, 3 | 0x44FDD0 | default, but not while bombing |
| | 4 | 0x44FE20 | laser: not while bombing nor when 0x164D0B4 bit 13; one live laser per slot (phase = slot); slot timer = 999 |
| | 5 | 0x44FFA0 | default, then aim at +0xE2AB0 if its x > −100 (speed × 1.5) |
| | 6 | 0x450080 | angle = wrap(option base angle +0xE2B0C + entry angle + π/2), speed |
| | 7 | 0x4501B0 | phase test as init 0; **only when it fires**: `Player_InitShot`, then angle (+0x450) = Rng_Signed × π/48 − π/2 (**RNG**, one draw per fired bullet; no draw on a non-matching frame), vel = polar(angle, entry speed) (VERIFIED 0x4501B0, RE-Q36) |
| | 8 | 0x450110 | not while bombing; angle = wrap(option angle (+0xCD0) + entry angle) |
| update | 1 | 0x450320 | homing (§4.4) |
| | 3 | 0x450580 | only while the bullet state (+0x462) == 1 (flying): vy (+0x440) −= `Rng_FloatRange`(0.1) + 0.27 (**RNG**); in any other state (hit animation) nothing, no draw (VERIFIED, RE-Q24) |
| | 4 | 0x4505D0 | laser body (follows the option/emitter, length from timer). **No shipped entry uses update 4** (sht_dump of all 8 files: update indexes used are 0, 1, 3, 5 only; VERIFIED RE-Q36), so it is unreachable |
| | 5 | 0x450840 | laser that follows option 0 with a 32-segment trail and creates damage rectangles |
| draw | 1 | 0x450AD0 | trail draw |
| hit | 1 | 0x450C50 | explosion: first hit → hitbox enlarged by sprite (48–128 px), vel = polar(Rng × π/2 − 3π/4, 6) (**RNG**); in hit state only every 2nd frame (4th during a spell card) damages, damage /= 3 (min 1), vel × 0.88. The RNG draw happens on the first hit regardless of the sprite. After either branch, on non-filtered frames only: bullet timer % 6 == 0 → `Effect_Spawn(5, enemy pos, 1, 0xFFFFFFFF)` (so effectively every 6th frame, every 12th during a spell card) (changelog/answers_impl10.md §8, VERIFIED) |
| | 2 | 0x450EE0 | u8 counter player +0xE2A94 (shared with the damage-area hit effect, §4.5) += 1 per call; when it is ≡ 0 mod 8: effect type 5 at (bullet pos.x, enemy pos.y, enemy pos.z); returns 0, so it never filters (VERIFIED, RE-Q24) |

## 3. Input and movement (`Player_Move`)
Input word 0x164D52C (previous frame 0x164D534): bit 0 shot, 1 bomb, 2 focus, 4 up, 5 down, 6 left, 7 right.
Direction code (+0xE2A98): up+left 5, down+left 7, up+right 6, down+right 8, else down 2, up 1, left 3,
right 4, none 0 (checked in this priority order).

Focus: f = (bombing ? bomb index & 1 : input & 4). On a change of f: options are swapped (§5), sprite script
reset (teams: 0 human / 5 youkai, with an effect if the previous state lasted > 3 frames), the youkai aura effect
created/removed, focus age := 0, gauge timer C := 0. Otherwise focus age += 1. When focus age > 6: mode
byte +5 := f. Solos override +5 with their parity every frame.

Velocity per frame: orthogonal speed or diagonal component from the sht fields (§2.1), × the speed
multipliers (+0x404 x, +0x408 y). Sprite script on horizontal start/stop (teams by focus, solos by parity):
human set 1 (start left: vx < 0 and previous vx ≥ 0), 2 (stop from left), 3 (start right), 4 (stop from right);
youkai set 6/7/8/9. "Previous vx" is the float player +0xE2A9C, written with this frame's vx at the end of the
movement; the focus-switch sprite-script reset (teams, script 0 / 5) also sets +0xE2A9C := 0, so moving on the
switch frame replays the start-move script (changelog/answers_impl18.md §Q3, VERIFIED code).
pos += v × gs; clamp x to [8, 376] and y to [16, 432] (game-manager +0x3DDE4/+0x3DDE8 = 8/16, ranges
+0x3DDEC/+0x3DDF0 = 368/416). Then the three boxes are recomputed. Options run (§5). Shot button: shot timer
starts (§4.2) if no dialogue and not blocked. Gauge drift (§9.3). Trail shifted if moving.

Boxes (half sizes from the human sht): hitbox ±(hit/2) — e.g. Reimu 0.825 → a 1.65 px square; graze ±(graze/2)
(the bullet box is additionally enlarged by 20 px each side in the graze test, lasers by 48); item box
±(collect/2).
Writers of the hit / graze boxes +0x38C…+0x3B8 (VERIFIED RE-Q31, byte scan for every displacement/`add` of these offsets): only `Player_Move` (0x44BC0A…0x44BCA9) at runtime, plus the zeroing in `Player_Register` (`rep stosd`; the static constructor 0x449CA0 runs no-op Vec3 constructors). No respawn, stage-start or bomb code writes them: before the first `Player_Move` of a stage they are 0, and in states 1/2 (no `Player_Move`) they keep the box of the last moved frame (the death position), even though the respawn sets a new pos.

## 4. Shooting

### 4.1 Player bullets (128 × 0x484 at +0xBE838)
+0 AnmVm; +0x2A4 pos; +0x430 hitbox w,h; +0x43C vel; +0x444 offset (laser); +0x44C speed; +0x450 angle;
+0x454 Timer; +0x460 i16 damage; +0x462 i16 state (0 free, 1 flying, 2 hit animation); +0x464 i16 type;
+0x466 laser slot; +0x468, +0x46A (trail length); +0x46C focus at fire; +0x46E script; +0x470 red tint;
+0x474 update fn; +0x478 extra draw fn; +0x47C hit filter; +0x480 entry.

### 4.2 Shot timer (0x451500, after movement)
Only when the gameplay frame counter 0x164D2C8 ≥ 20 (11 §2: GM+0x3DDC0, +1 per non-paused gameplay calc, reset at GM register/remove) and the shot timer ≥ 0 and not in
the "special bomb" (index 4): if the timer value changed since last frame (prev ≠ cur) and not (bombing with
shot types 1, 6, 7): fire with t = timer value. Tick; if t > 19 → −1. If shot held and timer < 0 and no
dialogue → 0. In states 1/2 → −1. `Player_Move` also sets it to 0 on shot press.
So a held shot button gives a continuous 20-frame cycle t = 0..19.

### 4.3 Firing (0x450F60)
sht = youkai sht if focused else human sht; level (§2.1). Walk free bullet slots in index order together with
the entry list: for the current entry call its init(slot, t, entry); result 1 → the slot is taken (VM flag
0x2000, state 1, fns copied), next slot + next entry; 0 → same slot, next entry; stop at list end.
Default spawn (0x44FB70): pos = player (origin 0) or option (origin k: the option's pos field +0x6B0 + (k − 1)·0x2F4, read whatever the option's state — there is no free/active test, a free option gives its last stored pos; VERIFIED 0x44FB70 and `Player_Fire` 0x450F60, RE-Q24) + offset, z 0.495; hitbox, angle, speed;
vel = (cos, sin)·speed; timer 0; focus copy; type, damage, script; sound; VM script = script + 10; red tint if
side > 0 and gauge ≥ youkai max.

### 4.4 Player bullet update (0x451150, skipped when 0x164D0B4 bit 10)
For each non-free bullet: update fn (if any) returning ≠ 0 → free. Else pos += vel × gs; unless types 4/5:
off-screen (sprite box, 08 §4.3) → free; run VM, free when it ends; tick timer.
Homing (update 1): while state 1, a target exists (x > −100), timer ≤ 39 and the timer advanced: d = target −
pos; k = max(1, |d| / (speed/4)); v' = v + d/k; speed = clamp(|v'|, 1, 10); v = v'·speed/|v'|. Otherwise if
speed < 10: speed += 1/3 and v rescaled. angle = atan2(v) every frame.

### 4.5 Damage dealt to enemies (`Player_DamageAgainst` 0x451670)
Called by the enemy manager with an enemy box (pos, size), an accumulator (the boss time-orb counter) and an
optional "hit by bomb" output. Returns 0 if the player state timer did not advance this frame. Else:
- every bullet that is active and (state 1 or type 3), overlaps the box, (types 4/5: timer even), and whose hit
  filter (if any) returns 0 ("overlaps": both boxes are built as f32 edges by helper 0x451CE0, min = c − s·0.5,
  max = s·0.5 + c, then bullet min.y ≤ box max.y, bullet min.x ≤ box max.x, box min.y ≤ bullet max.y, box min.x ≤
  bullet max.x, inclusive; not |d| ≤ sum of halves — VERIFIED 0x451670, RE-Q30): damage += bombing ? max(1, dmg/5) : dmg; while accumulator ≥ +0xE2B2C:
  accumulator −= step and, at human max gauge with a human-side shot, a time orb (type 7) spawns at the bullet;
  unless types 4/5/6 the bullet enters state 2 and (not type 3) vel /= 8. Entering state 2 (only if it was in
  state 1): save VM rotation z (vm+0x08); restart the VM with script (entry script + 11) on the player ANM
  file (this re-initialises the whole VM: scale, colour, tag, timers); **restore rotation z**; effect 5 at the
  bullet; pos z = 0.1. The bullet angle (+0x450) is not changed. Exact order (VERIFIED 0x451670, RE-Q24): rotation saved →
  `AnmFile_SetScript` 0x4069F0 (sets file/script, `AnmFile_StartScript`: `AnmVm_Init` + one `AnmVm_Run`; unlike
  `AnmManager_SetScript` it does **not** zero the VM pos / pos offset, and the owner writes neither afterwards) → rotation
  restored → `Effect_Spawn(5, bullet pos, 1)` (its 2 RNG draws) → bullet pos z := 0.1 → state := 2. VM flag 0x2000
  (z-write off) is **not** re-set by the code: `AnmVm_Init` resets the low flag bits, so the hit VM has bit 13 only if
  its own script runs `zwrite_off` (most player hit scripts do at time 0). No player*.anm script draws RNG, so the
  restart itself never touches the RNG. VERIFIED (`Player_DamageAgainst` 0x451670; changelog/answers_bugs1.md B1).
  The hit scripts (e.g. player00.anm 15/17) set tag 1 again, so the hit-pass draw (14 §5) keeps setting
  rotation z = bullet angle every frame: hit animations are oriented along the shot direction.
- after the bullet loop: accumulator += min(shot sum, 50). The **returned damage is not capped** (only the
  accumulator increment is); the orb `while` loop runs inside the bullet loop, per hitting bullet, against the
  accumulator value from earlier frames, before this frame's add. The accumulator is the enemy's own e+0x2E10,
  which starts at the step (07 §4.3 +0x2E10) (VERIFIED, changelog/answers_impl16.md §1–3).
- damage areas (first 192, §8) whose life % period == 0 and that overlap: damage += area damage; the area's
  dealt total grows and is capped by its max (excess removed, area damage := 0 when reached); "hit by bomb" := 1
  while bombing. "Overlap" with enemy pos p and hitbox size s (VERIFIED 0x451670, RE-Q24):
  - circle (r ≠ 0): **point test on the enemy centre only**, (a.y − p.y)² + (a.x − p.x)² ≤ r² (the box is ignored);
  - rect, rotation 0: inclusive AABB overlap, with box min = p − s·0.5 and max = s·0.5 + p (helper 0x451CE0):
    a.x − w/2 ≤ max.x, min.x ≤ w/2 + a.x, a.y − h/2 ≤ max.y, min.y ≤ h/2 + a.y;
  - rect, rotation ≠ 0: d = p − a rotated by −rotation (`Vec2_Rotate`), then −w/2 ≤ s.x/2 + d.x, d.x − s.x/2 ≤ w/2,
    and the same for y with h, s.y (the box is not rotated).
  Hit effect: unless the area's no-effect byte +0x3D is set, the u8 counter +0xE2A94 += 1, and when it is ≡ 0 mod 4
  effect type 3 spawns at p (the type-5 branch for index ≥ 192 is unreachable).
- at youkai max gauge: damage = damage × 106 / 100 (integer).
The enemy side then applies its own caps/scaling (07 §10.1).

## 5. Options (familiars)
4 slots × 0x2F4 at +0x40C: +0 AnmVm, +0x2A4 pos (shot origin), +0x2B0 anchor, +0x2C8 state (0 free, 1 init,
2 active, 3 leaving), +0x2CC sub-state, +0x2D0 index, +0x2D8 angle, +0x2E0 Timer, +0x2EC calc fn, +0x2F0 draw fn.
Each active option: calc fn, VM run, timer tick (in `Player_Move`). State 3: interrupt 5 at t == 0, freed when
t > 16.

Sets (calc table 0x4C7D40, draw table 0x4C7E10, 4 per shot type): solos get theirs at load; teams get the youkai
set when focusing and lose it (state 3) when unfocusing; team 3 (Youmu & Yuyuko) additionally gets option 2 =
0x44F2D0 (human side) on unfocus.

| shot | options |
|---|---|
| 0, 5 (Yukari) | 1 × 0x44E3A0: appears above the player (y − C, min 32), follows / attacks the aimed enemy (helpers 0x44E770 / 0x44E8D0) |
| 1, 7 (Alice) | 1 × 0x44EA40: at (player.x, player.y − 32), stays put while bombing |
| 2 (team, Remilia) | 4 × 0x44EB70: anchor fixed at spawn: (−30,−16), (−10,−32), (+10,−32), (+30,−16) from the player; after 12 frames angle += +π/120, −π/90, +π/90, −π/120 per frame; pos = anchor + polar(angle, 8) |
| 9 (Remilia solo) | 4 × 0x44EE70 (not detailed) |
| 3, 11 (Yuyuko) | 2 × 0x44F930: anchor += (target − anchor) × 0.09 with target = player ∓ (32, 0); angle ±π/60 per frame (start 0 / −π); pos = anchor + polar(angle, 6); effect 0x2F every frame |
| 3 human side | option 2 = 0x44F2D0 (Youmu's half, not detailed) |
| 10 (Youmu solo) | option 2 = 0x44F5E0 (not detailed) |
| 4, 6, 8 | none |

## 6. Death, deathbomb, respawn

### 6.1 Getting hit (`Player_Die` 0x44AB40), from a bullet/laser/enemy hit while state 0
Freeze byte 0x160F534 := 0; checksum update (RNG, §9.4); effect; state 2, state timer 0; sound 4.
Normal mode (0x164D0B4 bits 7–8 clear): gauge := 0; deathbomb counter n:
- bombs < 1: n = 2 ("Miss");
- else n = bombs × 6, +7 if time (stats+0x3C) ≥ stats+0x40, cap 15; during a boss spell card n × 2 (cap 30);
  shot types 0, 4, 5: n = n × 9 / 5; the player VM's colour2 := (R, G, B) 0xFFFFFF with A = the VM colour's
  alpha and VM flags bit 17 set (draw with colour2, 05 §2.1; cleared at a normal bomb start §7 and at the real
  death §6.2; VERIFIED 0x44AB40, RE-Q38); a ring effect is shown (`Effect_SpawnFixed` 0x3B, pointer kept in +0xE2B28); during a spell card bullets are frozen (0x164D0B4
  bit 10 → `BulletMgr_Calc` skipped).
Special mode (bits 7–8 set): n = 2 and forced bomb (+6 = 1, §7 "dissolve spell").
All items in state 1 fall again: `ItemMgr_DropAttracted` 0x441530, called as the last step of `Player_Die` (both modes) and again at the real death (`Player_DyingUpdate`, after the death drops, §6.2), walks the active item list and for every item with state +0x2D7 == 1 sets state := 0 and vel (+0x2B0) := (0, −0.9, 0). This is not the −0.7 of the `ItemMgr_Calc` player-dying branch (08 §8.4), which only applies to items still in state 1 when the calc runs (VERIFIED 0x441530, RE-Q24; runtime: attract demo 0, player hit at 4547 above the collection line with the auto-collect condition true, point item 428 stops at x 174.3 instead of drifting to 300, RE-Q28).
Before this, the hit test itself calls `GameMgr_ScrambleDecoys` (5 × Rng_Range(100000) + 3 × Rng_Float…; 08 §4).

**Work before `Player_Die`** (RE_NOCLIP, VERIFIED). The callers are `Player_HitTest` 0x44A230 (bullets),
`Player_HitTestNoBomb` 0x44A360 (enemy contact) and `Player_LaserTest` 0x44A6A0 (lasers). In order:
1. player+0xE2A90 := 6 (HitTest, HitTestNoBomb).
2. HitTest only: the bomb-area test. Inside a bomb area → return 2, nothing else.
3. The hit-box overlap test. LaserTest has its own geometry and graze path. No overlap → return 0.
4. The replay statistics word (0x18B8A28)+0xDA |= 2. It is not a counter.
5. **if player state (byte +0) ≠ 0 → return 1** (LaserTest: 0). The decoys are **not** scrambled in this case.
6. State 0: `GameMgr_ScrambleDecoys` (5 Rng_Range + 3 Rng_FloatRange = 11 draws), then `Player_Die`, return 1.
   No caller uses the LaserTest return value.

`Player_LaserTest`(centre c, size s, origin o, angle a, graze flag) geometry (VERIFIED 0x44A6A0, RE-Q30): q = o + (player pos − o) rotated by −a (`Vec2_Rotate`: sin/cos of −a rounded to f32; x' = cos·x − sin·y, y' = sin·x + cos·y); player box q ± hit half-extents (+0x3D4); laser box c ± s/2. Miss when lmax.x < pmin.x or pmax.x < lmin.x or lmax.y < pmin.y or pmax.y < lmin.y (f32 edges, touching counts as overlap). Overlap → statistics bit, then step 5/6 (state 0: scramble, `Player_Die`, return 1; else return 0). Miss and graze flag set: laser box grown by 48 on every side, tested again; overlap and player state not 1/2 → `Player_OnGraze`(player pos, laser flag), return 2; else 0. Callers: lasers (08 §6.3, c = (origin.x + tail + L/2, origin.y)) and ECL natives 9/11/25 (07 §12).

`Player_Die` itself starts with a debug print, then freeze byte 0x160F534 := 0, then `GameMgr_UpdateChecksum` (4 draws).
So an invulnerability cheat that "returns at the start of `Player_Die`" keeps steps 1–6, including the 11 decoy draws,
and skips everything in this section's first paragraph. That is what the noclip session mode does (probe: `Player_Die`
→ ret).

QUIRK: the original's own state ≠ 0 check (step 5) comes *before* the scramble. A port that places its
state check after the scramble consumes 11 extra draws on every hit during invulnerability. Keep the order above for
normal play.

Side effect of skipping `Player_Die`: the decoys stay scrambled without the matching checksum update, so
`GameMgr_IsTampered` 0x40BB80 reports tampering until the next `GameMgr_UpdateChecksum`. In the original that refuses
bombs (`Player_Move`), power/lives/bomb adds, and quits from `EffectMgr_Calc` at its next frame-counter ≡ 100 (mod 300).
The noclip probe therefore also forces `GameMgr_IsTampered` to return 0. That check is pure and returns 0 in every
untampered run, so this matches a port that ignores the anti-tamper checks (13 §2.1, 11 §4 step 7).

### 6.2 Dying state (state 2, `Player_DyingUpdate`)
While n > 0 (every frame): time −15 (0x418220), n −= 1, last-spell request (+4) := 1. A bomb pressed now
(§7) becomes a last spell and saves the player (the bomb sets state 3). When n reaches 0 (the real death):
ring effect +0xE2B28 ended (byte +0x350 := 0, pointer 0), effects + sound 0xF; bullets unfrozen (GameFlags bit 10 off,
`AnmMgr_TintOff` — the only tint-off of `Player_DyingUpdate`, not every dying frame); player VM flags bit 17 cleared; byte 0x164D0B0 := 0 (here, after the
statistics bit +0xDA |= 4 — not in `Player_Die`, so a deathbomb leaves it unchanged; 11 §2, VERIFIED 0x44CBF0, RE-Q36);
spell-card failure notified; miss counter; time −= min(time/10, 500)
(as −(time)/10 when time ≤ 5000); drops: lives < 1 → power := 0 and 5 full-power items; else power :=
0 if ≤ 16 else power − 16, 1 big + 5 small power items, and for shot types 2, 8, 9 with bombs > 0 a bomb
item (all mode 2); attracted items fall; **rank −1600**.
When n == 0 at entry (after death): shrink animation (scale y = 1 + 3t/30, x = 1 − t/30, t = the state timer +0xE2AF4; **that timer is not reset at the real death**: it was set to 0 by `Player_Die` and has ticked once per `Player_Calc` ever since (`Player_UpdateInvuln` ticks it in every state except 3), so t counts from the hit, the shrink only covers what is left of the 30 frames after the window, and when the deathbomb window lasted ≥ 30 frames the respawn happens on the first frame with n == 0 at entry, with no visible shrink. Example (attract demo 2): hit → state 2 at frame 2907, real death 2922, state 1 at 2938 (VERIFIED 0x44CBF0, 0x44D2C0, runtime rngsite demo 2, RE-Q24); every such frame
also last vx/vy +0xE2A9C/+0xE2AA0 := 0); at t > 29:
state 1, pos = (192, 448 − 64), timer 0, scale 3, idle script; if lives > 0: lives −= 1, bombs :=
trunc(sht bombs) (3); respawn starts (returns 1); else game over (0x164D0BB = 1) — lives and **bombs are not
changed** on game over (VERIFIED 0x44CBF0, RE-Q24).

### 6.3 Respawn (state 1, `Player_RespawnUpdate`)
Clear area for 60 frames (+0xE2A70 := 60 on **every** `Player_RespawnUpdate` call, so it counts down from 60 only after
the respawn ends; `Player_UpdateInvuln` decrements it while ≠ 0 and adds, each frame a full-screen cancel rectangle 768 × 896 at the player
with item −1, i.e. bullets vanish without items). Grow animation over 30 frames, speed multipliers 1, alpha =
t × 255 / 30, deathbomb counter 0. At t > 29: state 3, scale 1, colour white; invulnerability timer = 240
(with 0x164D0B4 bit 14, spell practice, the timer is not set and keeps its value 30, so the invulnerability lasts
30 frames; VERIFIED 0x44D180); counter := sht +8 (bombing allowed). Since `Player_UpdateBomb` runs before the state update in `Player_Calc` and the counter is 0 throughout state 1 (it reached 0 in state 2 and the respawn rewrites 0 every frame), **no bomb can start in state 1**; the first possible start is the first state-3 frame (VERIFIED 0x44C390, 0x44D180, RE-Q24).

### 6.4 Invulnerability (state 3)
`Player_UpdateInvuln` 0x44D2C0 order (VERIFIED RE-Q27): (1) respawn clear first: if +0xE2A70 ≠ 0 → −= 1 and the
768 × 896 cancel rectangle (§6.3); (2) then state 3: byte +4 := 0, the invulnerability ring (+0xE2B1C, if any) moved to
the player pos, timer decremented; t < 1 → ring freed, state 0, timer := 0, colour white; else colour 0xFFF02020 when
(t mod 8) < 2 else white; (3) any other state: the timer (+0xE2AF4) ticks.
Timer decremented each frame; colour 0xFFF02020 when (t mod 8) < 2 else white; at t < 1: state 0. In other
states the timer just ticks. Bombs set state 3 with their own invulnerability time (§7). At load (`Player_Load`
0x44D650, VERIFIED RE-Q24): pos = (playfield w / 2, playfield h − 64, 0.49) = (192, 384, 0.49) (the respawn uses z 0.2,
§6.2); state 1 and timer 120, or 10 in spell practice (game-manager flag +0x3DBAC bit 14).

## 7. Bombs (`Player_UpdateBomb` 0x44C650)
Start conditions (when not bombing): forced (+6 set and counter == 1), or: bomb **held** (bit 1 set in the current
game input word g_Input 0x164D52C — a level test with **no** edge test against the previous word, VERIFIED asm
0x44C85A, RE-Q36; so holding the bomb button starts a new bomb on the first non-bombing call whose other tests pass,
e.g. the frame after a bomb ends, or the first frame of a deathbomb window when it was already held at the hit), not
blocked by game manager / dialogue, counter (+0xE2A68) ≠ 0, bombs ≥ 1, cooldown 0. In the special mode
(0x164D0B4 bits 7–8 or bit 14), a call that passes those tests only plays sound 0x29, and only when bit 1 is newly
set (g_Input bit 1 ≠ g_InputPrev 0x164D534 bit 1: the only edge test of the function); it returns either way.
Start: statistics bit; +6 := 0; then the cost block runs **only in normal mode** (GameFlags bits 7–8 clear; bit 14 is
not tested here). In special mode (bits 7–8 set: reachable only through the forced start after a hit, §6.1, e.g. a
death during a boss last spell) the whole block is skipped: index := 4 and nothing else — no bomb cost, no
`SetBombs`/`AddBombs` (so none of their checksum RNG draws), no counter 0x164CFA8 / 0x164CFAC, no
`GameMgr_AddBombCount` 0x44E2E0 (no checksum draws), no unfreeze / ring removal / tint off, +0xFEC unchanged
(VERIFIED 0x44C650, RE-Q36). Normal mode, in this order (VERIFIED 0x44C650, RE-Q38): player VM (+0x10) flags
bit 17 cleared (the colour2 silhouette of §6.1 off); deathbomb ring effect +0xE2B28, if any: its byte +0x350 := 0
(effect ends) and the pointer := 0; GameFlags bit 10 cleared (bullets unfrozen); **`AnmMgr_TintOff`** 0x40BAB0
(AnmManager+4 := 0, tint colour 0x80808080; 06 §10.2); then index = focus; if +4 (last
spell) index = 1 − focus; no last spell: bombs −= 1 (counter 0x164CFA8); last spell: index += 2, uses all bombs
if forced or bombs < 2 (count kept in +0xFEC, bombs := 0) else 2 bombs; counter 0x164CFAC; game-manager
0x44E2E0(1). Special mode: index 4. Then +4 := 0; bombing := 1; +0xE2A7C := 1; timer 0; duration := 999 (the bomb
function sets the real value on its first frame via 0x40BE30); bomb calc fn runs immediately; **rank −200**;
spell manager notified; counter += 6 (cap sht +8).
Each frame while bombing: if timer < duration: bomb calc fn, tick. Else end: spell manager notified, speed
multipliers 1, bombing := 0; index 4 also clears GameFlags bits 7–8 and, for every boss slot, kills its chained
children without reward, sets the boss life (+0x2DFC) to 0 and clears flag A bit 30; then a white flash (3: 30, 1,
0xFFFFFFFF) (14 §2.17, changelog/answers_impl11.md §14, VERIFIED).
Gauge while bombing (indexes 0–3): odd index += 26000/duration per frame, even −= 26000/duration (integer).
Added only in the "already bombing" branch of `Player_UpdateBomb`, after the calc-fn/end test — so **not on the
start frame** (that branch returns after the start), but **also on the end frame** (timer ≥ duration: the end
handling runs, then the gauge add still happens with the old index) (changelog/answers_impl18.md §Q3, VERIFIED code; trace
selftest1 frame 684: no change, then +173 per frame). Forced add (`GaugeAdd(v, 1)`), so not skipped while bombing.
Indexes: 0 human bomb, 1 youkai bomb, 2 human last spell, 3 youkai last spell, 4 "dissolve spell".
Bomb start helper 0x40BE30(name, duration, invulnerability, isLastSpell): declares the spell name, sets the
duration, **state 3 with the invulnerability timer**, invulnerability ring effect, clears bomb objects, all
items attracted (state 1, vel (0, −0.5)), stores the origin.

Per bomb (names from the exe strings; areas: cancel C = bullet-cancel circle/rect, D = damage circle/rect with
damage per hit, period +0x38, max total +0x34; sizes in px):

| owner (table) | name | dur | invuln | speed mult (values written; timing per phase in 14 §2, VERIFIED) | areas (main) |
|---|---|---|---|---|---|
| Reimu 0x40C010 | 霊符「夢想妙珠」 | 200 | 260 | — | at start C r 96 at the player (200 f); 16 orbs each with D r 64 dmg 5 period 2 max 200 (life 200) and, while flying, a one-frame C r 128; on impact C r 64 (+4.27/f, 30 f) and D r 64 (+12.8/f) dmg 500 life 12 period 4 |
| Reimu LS 0x40C910 | 神霊「夢想封印　瞬」 | 200 | 260 | — | 16 orbs as above; impact D r 64 (+8.53/f) dmg 25 life 15 period 5 max 50; from frame 40 every 20 frames a blast at the target (or a random point, **RNG**): C r 64 (+4.27/f, 30 f), D r 64 (+8.53/f) dmg 400 life 15 period 2 |
| Yukari 0x410C40 | 境符「四重結界」 | 150 | 200 | — | 4 × (C r 100 grow 1/f, 40 f; D r 100 grow 1/f dmg 70 life 40 period 5) |
| Yukari LS 0x410FE0 | 境界「永夜四重結界」 | 250 | 300 | — | as above, cancel life 40/100 |
| Marisa 0x40E3B0 | 恋符「マスタースパーク」 | 300 | 350 | 0.2 | C rect 384 × len; D rect 128 × len dmg 12, D rect 384 × len dmg 6 |
| Marisa LS 0x40E780 | 魔砲「ファイナルスパーク」 | 350 | 380 | 0.2 | as above, dmg 12 / 7 |
| Alice 0x40D430 | 魔符「アーティフルサクリファイス」 | 210 | 250 | — | at option: C r 1 (+5/f, 110 f), D r 1 (+5/f) dmg 70 life 110 period 5; C/D r 32 dmg 40 |
| Alice LS 0x40D970 | 魔操「リターンイナニメトネス」 | 230 | 280 | — | same areas |
| Sakuya 0x40FCD0 | 幻符「殺人ドール」 | 250 | 290 | 0.5 | knives: C r 32 (500 f), D r 32 dmg 20 life 500 |
| Sakuya LS 0x4103F0 | 幻葬「夜霧の幻影殺人鬼」 | 320 | 350 | 0.5 | knives: dmg 30 |
| Remilia 0x40EE10 | 紅符「不夜城レッド」 | 240 | 290 | 0 then 2 | C rects 96 × 800 and 800 × 96, C r 96 |
| Remilia LS 0x40F570 | 紅魔「スカーレットデビル」 | 280 | 320 | 0 then 3 | same cancel shapes |
| Youmu 0x411B10 | 人符「現世斬」 | 220 | 270 | 0 then 1 | C rects 96 × 448 (60 f); D rects 96 × 448 dmg 80/100/300, periods 2–5 |
| Youmu LS 0x4123D0 | 人鬼「未来永劫斬」 | 250 | 300 | 0 then 1 | 7 slash rows: D rects dmg 100…40, final 500 |
| Yuyuko 0x413140 | 死符「ギャストリドリーム」 | 300 | 350 | 0.8 | 4 × (C r 24 500 f; D r 24 dmg 50 max 800) |
| Yuyuko LS 0x413990 | 死蝶「華胥の永眠」 | 300 | 350 | 0.8 | as above + C r 64; one D max 1200 |
| special 0x40D100 | 「ディゾルブスペル」 | 120 (40 if gm flag) | 200 | 0 | screen effect; boss lock removed |

Table 0x4C7AD0 per shot type (calc fns 0–4, draw fns 5–9): teams combine the human member's bomb/LS (0/2)
and the youkai member's (1/3); solos use their own for both focus states. Bomb object choreography (orb homing,
slash timing, etc.) is only summarised here (INFERRED detail level); the duration/invulnerability/area numbers
above are VERIFIED from the call arguments.

## 8. Areas (0x40 each)
| off | meaning |
|---|---|
| +0 / +4 | x, y |
| +8 | radius (0 → rectangle) |
| +0xC | radius growth per frame |
| +0x10 / +0x14 | rectangle w, h |
| +0x18 / +0x1C | w, h growth per frame |
| +0x20 | rotation (rectangle tested in the rotated frame) |
| +0x24 | life (frames; decremented while ≥ 0, freed when it drops below 1) |
| +0x28 | cancel item type (cancel areas; 6 default, −1 none) |
| +0x2C | damage per hit (damage areas) |
| +0x30 | hit counter (cancel) / dealt total (damage) |
| +0x34 | max total damage (0 = unlimited) |
| +0x38 | period (default 1): damages when life % period == 0 |
| +0x3C | u8 active; +0x3D u8 no hit effect |

8.1 Update (start of `Player_Calc`, all 384): life ≥ 0 → life −= 1, radius/w/h grow; life < 1 → freed. An area
created with life 0 therefore exists for exactly one bullet/enemy pass.
Creators: cancel circle 0x44DF00(pos, r, dr, life, item), cancel rect 0x44DE60(pos, w, h, item, life), damage
circle 0x44E040(pos, r, dr, dmg, life), damage rect 0x44DFA0(pos, w, h, dmg, life); slot = first inactive of
the half (index 191 reused when full).
Bullet test (`Player_InBombArea`, cancel half): circle → distance² < r²; rect → inside (rotated) bounds;
hit: player +0xE2A90 := area item, hit counter += 1, returns 2; a miss returns 0 and **does not touch +0xE2A90**
(only the hit/graze tests reset it to 6 first; 08 §4.1; VERIFIED RE-Q27).

## 9. Lives, bombs, power, gauge, scoring hooks

### 9.1 Stats block (game manager +8; full layout in 11)
+4 graze (cap 99 999) · +8 score / 10 · +0xC graze total (cap 999 999) · +0x20/+0x22 i16 gauge (copy/current) ·
+0x24 max point value · +0x2C/+0x30 point-item counters · +0x34 extends · +0x38 next extend · +0x3C time ·
+0x40 time threshold · +0x44 time total · +0x48.. decoys · +0x74 f32 lives · +0x80 f32 bombs · +0x98 f32 power.

- Lives / bombs ≤ 8 via items; extend rule 08 §8.5. On continue: lives := settings byte (+0x1C of the settings
  object 0x160F50C), bombs reset, power 0, next extend 100 (0x404890; full flow in 11).
- Power: float 0–128; +1/+8 from items, −16 on death (0 if ≤ 16), 0 when out of lives.

### 9.2 Graze (`Player_OnGraze` 0x44A930, bullet or laser)
Only when not bombing (+0xFDC == 0 — global 0x17D6ED4): g = 3 if gauge ≤ human max, 2 if ≤ human, else 1;
graze += g (cap 99 999), graze total += g (cap 999 999). Always: effect 8 at the midpoint, rank +6, HUD flag,
sound 0x1E (pan x), score += 2000 (4000 at gauge ≥ youkai): AddScore; if mode byte +5 (youkai): gauge +100 (not
while bombing). Time-orb drops: if (not solo human, or shot type 10) and a boss exists (any of 8 boss slots at
0x9DCDA0 + mgr) and gauge ≥ youkai max: one item type 10 (time orb, mode 5) at the bullet; if bullet (not
laser) and a spell card runs: a second and, still inside that branch, unless solo youkai a third — so a laser or a
no-card graze gives at most one orb (VERIFIED 0x44AACE–0x44AB14, RE-Q28). Statistics bit 0x2000.

### 9.3 Human/youkai gauge (stats +0x22, i16)
Limits/thresholds (game manager +0x3DDF8.., set at player load):

| | min | max | human max | youkai max | human | youkai |
|---|---|---|---|---|---|---|
| default | −10000 | 10000 | −8000 | 8000 | −2000 | 2000 |
| solo human | −10000 | 2000 | −8000 | 8000 | −2000 | 2001 |
| solo youkai | −2000 | 10000 | −8000 | 8000 | −2001 | 2000 |
| shot 3 (Youmu & Yuyuko) | −5000 | 10000 | −3000 | 8000 | −2000 | 2000 |
| shot 10 (Youmu) | −5000 | 5000 | −3000 | 3000 | −2000 | 2000 |

Queries: "human max" gauge ≤ human-max, "human" ≤ human, "youkai max" ≥ youkai-max, "youkai" ≥ youkai.
`GaugeAdd(n, force)` clamps to [min, max] and is ignored while bombing unless forced.
Per frame (in `Player_Move`, no dialogue, focus age ≥ 30, not bombing):
- shot timer ≥ 0 (shooting): timer B (+0xE2AD0) > 0 → B −= 1; else n = trunc(min(C/15, 21)) with C = timer
  +0xE2AE8 (C > 300 → 21), negative when unfocused (human); gauge += trunc(n × gs); C ticks.
- not shooting: B ≥ 4 → C := 0; B ≥ 30 → if |gauge| ≤ 9: gauge := 0, else drift toward 0 by 5 (beyond the
  max thresholds), 3 (beyond human/youkai), 2 (otherwise) × gs; else B ticks.
Bomb: ±26000/duration per frame (§7). Graze in youkai mode +100. Time orbs −111/+111 (08 §8.5). Death → 0.
At human max or youkai max an aura effect follows the player.

### 9.4 RNG side effects (important for replays)
`GameMgr_UpdateChecksum` 0x406E50 = 2 × Rng_Range(100000) (writes two decoy values and a checksum); it runs
on every change of lives (0x43C641, 0x406E20), bombs (0x439883, 0x406F30 does not), power (0x441850,
0x406FA0), time (0x418220, positive or negative unless clamped), and in `Player_Die`. The bullet/laser hit
path also calls `GameMgr_ScrambleDecoys` (5 + 3 RNG draws). Shot init 7, update 3 and hit filter 1 use the RNG.
