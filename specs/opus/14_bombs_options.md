# 14 — Bomb choreography, options, player laser, shot drawing

Sources: bomb table 0x4C7AD0 (09 §7), `Bomb_Start` 0x40BE30, `Player_SpawnInvulnRing` 0x40BF00, bomb tint
0x40BC60, per-bomb calc/draw functions (below), effect updates 0x40E040–0x40E2D0 / 0x410BB0 / 0x4114E0 /
0x4117B0 / 0x413070, option functions (tables 0x4C7D40 / 0x4C7E10), laser 0x44FE20 / 0x450840 / 0x450AD0,
player draw 0x44D530 / 0x44D630 (bullets 0x4512F0 / 0x451400). VERIFIED unless marked.
Notation: T = bomb timer (player +0xFF4, 0 on the first frame), "at T = n" = on the frame the timer
changes to n (the code tests "timer advanced and == n", so nothing fires twice in slow motion); p = player
position; R = `Rng_Float` (game RNG); areas: C = cancel area, D = damage area (09 §8: circle(pos, r, dr, …),
rect(pos, w, h, …); "life 0" = exists for one frame); effects = 13 §2.6; screen effects (type: p0, p1, p2) =
13 §3; colours ARGB.

## 1. Common machinery

### 1.1 Bomb objects (128 × 0x16F0 at player +0x1028)
| off | type | meaning |
|---|---|---|
| +0 | i32 | state (0 free, 1 active, 2 exploding) |
| +4 | i32 | counter |
| +8 | f32 | speed / orbit radius / angular speed (per bomb) |
| +0xC | f32 | speed (per bomb) |
| +0x10 | f32 | angle |
| +0x14 | f32[3] | position |
| +0x20 | f32[3] | centre / spawn point |
| +0x2C | f32[3] | last displacement (Yuyuko) |
| +0x1A0 | f32[3] | velocity / radius + radial speed (per bomb) |
| +0x1AC | f32 | spin (Sakuya) |
| +0x1B8 | AnmVm | sprite (player ANM file) |
| +0x440 | f32[3] | draw offset |
| +0x16DC | Timer | age (Sakuya) |
| +0x16E8 / +0x16EC | ptr | damage area / cancel area attached to the object |

`Bomb_Start` only sets every object's state to 0; all other fields keep their values from earlier bombs
(matters for Yuyuko's last spell, §2.8). The first object's +0x1B8 VM and the next ones' VMs are also used as
plain VM slots (player +0x11E0 + i × 0x2A4 = object 0 +0x1B8 …) by Marisa and Yukari.

### 1.2 `Bomb_Start(name, duration, invulnerability, isLastSpell)` (first frame of every bomb)
Spell-name banner (`Gui_ShowPlayerSpellName`, SE 0xE); player +0xFE4 = duration; player state 3 with
invulnerability timer; invulnerability ring (effect 23, fixed slot 0, 13 §2.6); all bomb objects state 0; all
items attracted; bomb origin (+0xB8828) = p. (09 §7 for the surrounding start/end logic.)

### 1.3 Background tint (0x40BC60, called first by every bomb draw function, EDX = colour C)
Per channel: T < 60 → 128 + (C − 128)·T/60; T ≥ duration − 60 → 128 + (C − 128)·(duration − T)/60;
else C. Alpha 0x80. The result goes to the stage tint (0x409160, 06) and a stage flag (0x4EA49C) is set.
| bomb | C | bomb | C |
|---|---|---|---|
| Reimu | 0x80404040 | Reimu LS | 0x802020D0 |
| Yukari | 0x80404040 | Yukari LS | 0x802020D0 |
| Marisa, Marisa LS | 0x80404040 | Alice | 0x80404040 |
| Alice LS | 0x802020D0 (+ white flash, §2.3) | Sakuya | 0x80404040 |
| Sakuya LS | 0x80202080 | Remilia | 0x80D02020 |
| Remilia LS | 0x80F00000 | Youmu, Youmu LS | 0x80404040, from T = 70 0x80000030 |
| Yuyuko | 0x80404040 | Yuyuko LS | 0x80802020 |
| dissolve | T < 60: 0x80404040, else grey g = (T − 60)·176/60 + 64 | | |

### 1.4 VM interpolation helpers used by bombs
0x40EC30 pos (slot 0: duration, mode, start xyz, end xyz) · 0x40EDA0 scale (slot 4) · 0x40ED50 alpha
(slot 2: duration, mode, start, end) · 0x40ECA0 colour (slot 1: duration, mode, start RGB, end RGB) — they
write the VM interpolation fields directly (05 §2/§6).

## 2. Bombs (calc function / draw function)

### 2.1 Reimu — 霊符「夢想妙珠」 (0x40C010 / 0x40C820), dur 200, invuln 260
- T = 0: effect 12 at p (0xFF4040FF); 16 orbs (objects 0–15): VM script 0x13, angle_i = −π + i·π/8,
  position = centre = p, radius (+8) = 0, state 1; per orb C circle(p, r 96, life 200, item 6) and
  D circle(orb, r 64, dmg 5, life 200, period 2, max 200, no hit effect) attached; SE 0xD.
- T < 40 (spin-out): angle += −π/60 (even i) / +π/60 (odd i); pos = centre + (sin a, cos a)·radius;
  radius += 3.2; velocity = pos − previous pos.
- T = 40: +0xC = |velocity|, angle = atan2(velocity), counter 0, speed (+8) = 8.
- T ≥ 40, state 1, timer advanced: target = homing target (+0xE2AA4) if its x > −100 else p; d = target − pos;
  k = max(1, |d| / (speed/8)); v' = d/k + velocity; speed = clamp(|v'|, 1, 10); velocity = v'·speed/|v'|;
  one-frame C circle(orb, r 128). Impact when the orb's D area has dealt ≥ its max (200) or T ≥ duration − 30:
  both areas off; C circle(**p**, r 64, dr 4.267, life 30); D circle(orb, r 64, dr 12.8, dmg 500, life 12,
  period 4, unlimited); effect 6 ×8; state 2; VM interrupt 1; SE 0xF @orb; shake (1: 16, 8, 0). (The code
  computes velocity/8 into a temporary and discards it — the orb keeps its speed. QUIRK)
- T ≥ 40 every frame (state ≠ 0): pos += velocity·gs. State 1: areas follow the orb. State 2: counter += 1 on
  advanced frames, freed after 30.
- Draw: orbs 0–15 (Draw2DSnap at pos + offset + shake, z 0).

### 2.2 Reimu LS — 神霊「夢想封印　瞬」 (0x40C910 / 0x40D010), dur 200, invuln 260
- T = 0: same 16 orbs and areas as 2.1; blast index (+0xFF0) = 0; SE 0xD.
- Every frame, state-1 orbs: angle ±π/60 as above, pos = centre + (sin, cos)·radius; while T < 40:
  radius += 2.4 (even) / 1.2 (odd). Orb i explodes when T ≥ duration − 40 − i: areas off; C circle(p, r 64,
  dr 4.267, life 30); D circle(orb, r 64, dr 8.533, dmg 25, life 15, period 5, max 50); effect 6 ×8; state 2
  (not freed; its VM keeps running); interrupt 1; SE 0xF @orb; shake (1: 16, 8, 0). Areas follow.
- Blasts: when T ≥ 40 and T % 20 == 0 (T = 40, 60, …, 180): object 16 + (+0xFF0), then +0xFF0 := 1 (so the
  first blast uses object 16 and every later one reuses object 17 — QUIRK); VM script 0x14; colour =
  table 0x4C6100[(T/20) % 7] = {0x8FFFFFFF, 0x8F0000FF, 0x8FFF00FF, 0x8FFF0000, 0x8FFFFF00, 0x8F00FF00,
  0x8F00FFFF}; point = homing target if x > −100 else (R·320 + 32, R·384 + 32) (**RNG**, x first);
  effects 0x31 and 0x37 (colour); C circle(point, r 64, dr 4.267, life 30); D circle(point, r 64, dr 8.533,
  dmg 400, life 15, period 2); SE 0xF; shake (1: 16, 8, 0); flash (3: 8, 1, colour).
- Objects 16–127: areas follow; freed when their VM ends. Draw: all active objects as 2.1.

### 2.3 Alice — 魔符「アーティフルサクリファイス」 (0x40D430 / 0x40D950), dur 210, invuln 250
The doll is **option 0** (player +0x40C; pos +0x6B0, VM rotation +0x414, alpha +0x5FF, state +0x6D4,
timer +0x6EC). Draw: tint only.
- T = 0: SE 0xD; start S = option position; SE 6.
- T < 60: doll pos = S + ((192, 224) − S)·(T/60)²; doll rotation −= π/10 per frame; one-frame C circle(doll,
  r 32) and D circle(doll, r 32, dmg 40).
- T ≥ 60: rotation 0, doll at (192, 224); T ≥ 150: doll alpha 0. Events: T = 60, 64, 68, 72, 76: effect 0x28
  colours 0xFFFFFFFF, 0xFFFFD0D0, 0xFFFFB0B0, 0xFFFF8080, 0xFFFF4040. T = 90: SE 0xF; effects 0x2A, 0x2B,
  0x2C; C circle(doll, r 1, dr 5, life 110); D circle(doll, r 1, dr 5, dmg 70, life 110, period 5); shake
  (1: 16, 8, 0); flash (3: 8, 1, 0x8FFFFFFF). T = 100, 110, 120, 130: effect 0x2D colours 0xFFFFFFFF,
  0xFFFFD0D0, 0xFFFF8080, 0xFFFF0000. T = 150: flash (3: 8, 1, 0x8FFFFFFF), shake (1: 16, 8, 0), SE 0x19.
  T = 209: option state := 1, option timer 0 (the doll re-initialises).
- Ring effects 0x2A–0x2D (k = 1 − (1 − t/40)², so they expand for 40 frames then collapse; life 80):
  0x2A wavy ring radius 256k, 64 segments, 5 waves, width 8 (t ≥ 40: amplitude 64k, width +2/frame);
  0x2B / 0x2C ellipses (semi-axes 256k × 128k) rotated +π/4 / −π/4, 48 segments, width 8 (t ≥ 40: +1.5/frame);
  0x2D octagon (8 segments) radius 192k, width 8.

### 2.4 Alice LS — 魔操「リターンイナニメトネス」 (0x40D970 / 0x40DEE0), dur 230, invuln 280
As 2.3 but: start S = **p** (no SE 6); doll alpha 0 from T ≥ 128; events: T = 60–76 effect 0x28 as above;
T = 120: SE 0xF, effects 0x2A–0x2C at the doll and effect 0x2D at (64, 96) 0xFF0000F0, (64, 352) 0xFFF00000,
(320, 352) 0xFF00F000, (320, 96) 0xFF00F0F0; C circle(doll, r 1, dr 5, life 110); D circle(doll, r 1, dr 5,
dmg 70, life 110, period 5) (no shake/flash); T = 130, 140, 150, 160: effect 0x2D colours as 2.3; T = 180:
flash (3: 8, 1, 0xFFFFFFFF), shake, SE 0x19; T = 229: doll re-initialises.
Draw: T < 90 tint 0x802020D0; 90 ≤ T ≤ 120: c = (T − 90)·208/30, tint (A 0x80, R (c/5 − 48) mod 256,
G c + 32, B c + 32) plus a white playfield rectangle with alpha (T − 90)·255/30; 120 < T ≤ 220: white
playfield rectangle alpha 0x70 (no tint call); T > 220: tint 0x802020D0.

### 2.5 Marisa — 恋符「マスタースパーク」 (0x40E3B0 / 0x40E610), dur 300, invuln 350
- T = 0: SE 0x13; object 0 pos = p; five beam VMs (player +0x11E0 + i·0x2A4) scripts 0x1E–0x22; speed
  multipliers 0.2; shake (7: 16, 120, 60, 120).
- Every advanced frame with T % 4 ≠ 0: C rect((192, p.y/2), 384 × p.y, item 6, life 0); D rect((p.x, p.y/2),
  128 × p.y, dmg 12, life 0, no hit effect); D rect((192, p.y/2), 384 × p.y, dmg 6, life 0, no hit effect).
  (Everything above the player is covered; the rectangles are not rotated.)
- The five VMs run every frame. Draw: tint; beam i (0–4): angle a = i·π/15 − π/2 − 2π/15 (fan of ±24°);
  pos = p + (cos a, sin a)·(sprite width·scale.x/2); rotation z = a; Draw2DRot (+ shake, z 0).

### 2.6 Marisa LS — 魔砲「ファイナルスパーク」 (0x40E780 / 0x40E610), dur 350, invuln 380
As 2.5 with scripts 0x23–0x27, SE 0xD + 0x13, wide D rect dmg 7 (narrow 12), plus: on advanced frames with
T % 10 == 0: ring effect 0x35 at p in fixed slot 4 + (n % 4) (n = counter +0xFF0, starts 0); odd n → VM script
0x5C instead of 88; 32 segments, 0 waves; pos interpolation 30 frames mode 4 (0,0,0) → (128,0,0) (radius
0 → 128); scale 30 frames mode 1 (32, 0) → (64, 0) (width); alpha 30 frames mode 3 255 → 0; colour
30 frames mode 0 white → 0xFF0000; VM run once; n += 1; SE 0x11 @p. (Effect update 0x427AE0 frees it at
alpha 0.)

### 2.7 Sakuya — 幻符「殺人ドール」 (0x40FCD0 / 0x410300), dur 250, invuln 290
- T = 0: knives (objects 0–95) state 0; speed multipliers 0.5; effect 20 at p (kept; follows p while T ≤ 60);
  SE 5.
- 20 ≤ T < 116, every frame: SE 6 and shake (1: 4, 1, 0). For each knife k in index order: at T =
  (k % 48)·2 + 20: if the knife is already active the whole function **returns** for this frame (QUIRK);
  else activate: VM script 0x16; angle = k·2π/96 − π; speed (+0xC) = R + 0.5; accel (+8) = R·0.1 + 0.03;
  spin = (Rng_Range16(1) == 0 ? −π/20 : +π/20) — always −π/20 (QUIRK; one RNG draw) (**3 RNG calls per knife**);
  pos = p + polar(angle, 24); age 0; C circle(knife, r 32, life 500); D circle(knife, r 32, dmg 20, life 500).
  Knives k and k + 48 appear together, two every second frame.
- Each active knife: age < 30 or age > 69: at age = 70 → if the homing target exists (x > −100) angle =
  atan2(target − pos); speed := 14. speed += accel; velocity = polar(angle, speed). 30 ≤ age ≤ 69: angle +=
  spin, velocity 0 (hover and spin). Areas follow; while age < 120: if its D area has dealt damage → VM
  script 0x17, effect 0 (0xFFFF80FF), areas off, SE 0x2B @knife; age ≥ 120: areas off. pos += velocity (not
  scaled by gs); VM run; age ticks. Knives are never freed before the next bomb.
- Draw: active knives 0–95, rotation z = angle, Draw2DRot.

### 2.8 Sakuya LS — 幻葬「夜霧の幻影殺人鬼」 (0x4103F0 / 0x410AC0), dur 320, invuln 350
As 2.7 but 128 knives; shake (1: 4, 1, 0) only once at T = 0; window 20 ≤ T < 148; knife k at
T = (k % 64)·2 + 20 with angle = wrap(k·2π/64 − π) and VM script 0x14; damage 30; at age 70 additionally
effect 0x2E (ring: radius +8/frame, 12 segments, width 32) at the knife; on hit: every knife with k % 3 == 0
also flashes (3: 2, 1, 0x208080FF); hit script 0x15. Draw function only draws knives 0–95 (knives 96–127
are invisible but still damage — QUIRK).

### 2.9 Remilia — 紅符「不夜城レッド」 (0x40EE10 / 0x40F550), dur 240, invuln 290
The bats are the 4 options; their anchors (option +0x2B0 = player +0x6BC + k·0x2F4) are moved:
- T = 0: SE 0xD; start anchors saved (objects 0–3); SE 6; counter 0; speed multipliers 0.
- T < 60: anchor_k = S_k + (A_k − S_k)·(T/60)² with A0 = (p.x − 32, p.y), A1 = (p.x, p.y − 32),
  A2 = (p.x, p.y + 32), A3 = (p.x + 32, p.y).
- T ≥ 60: at T = 60 speed multipliers 2 and interrupt 2 to the four option VMs; anchors := A_k every frame;
  C circle(p, r 96, life 0). Advanced frames with T % 10 == 0: ring effect 0x35 in slot 4 + (n % 4), 32
  segments, 4 waves; pos interpolation 30 frames mode 4 to (192, R·128, 0) (radius → 192, wave amplitude
  random, **RNG**); width constant 64; alpha 255 → 0 mode 3; colour white → red; n += 1; SE 0x11 @p.
  While the shot timer ≥ 5 (shooting): C rect(p, 96 × 800) and C rect(p, 800 × 96), life 0 (a cross).
  T = 239: the 4 options re-initialise (state 1, timers 0).
- No damage areas: the damage comes from the shot table — `ply02as.sht` levels 6/7 are used while a
  Remilia bomb has run ≥ 60 frames (09 §2.1).

### 2.10 Remilia LS — 紅魔「スカーレットデビル」 (0x40F570 / 0x40FCB0), dur 280, invuln 320
Identical to 2.9 except speed multipliers 3 at T = 60, ring width interpolates 64 → 128, options
re-initialise at T = 279.

### 2.11 Youmu — 人符「現世斬」 (0x411B10 / 0x412300), dur 220, invuln 270
- T = 0: SE 0xD; speed multipliers 0; player sprite script 0; SE 6; S = p, P = (p.x, 416).
- T < 40: player pos = S + (P − S)·(1 − (1 − T/40)²) (dash down). T = 40: effect 0x28 at p (0xFF8080FF).
- T = 70: player y := 32 (appears at the top); at (p.x, 224): C rect 96 × 448 life 60; D rect 96 × 448 dmg 300
  life 10; D rect dmg 80 life 60 period 5; effect 0x30 (0xFF8080FF); flash (3: 8, 1, 0xEFFFFFFF); SE 0x2A.
- T = 80 / 90 / 100: pairs at p.x ∓ 32 / 64 / 96 (y 224): C rect 96 × 448 life 60; D rect dmg 100 / 100 / 80
  life 40 (period 2 left, 3 right); effect 0x30 each; flash alpha 0xCF / 0xBF / 0x8F (white). At T = 100:
  Q := p.
- 120 ≤ T < 150: player pos = Q + (S − Q)·(1 − (1 − (T − 120)/30)²) (returns; the function exits early).
  T = 150: speed multipliers 1.
- Effect 0x30 (ring type 48, life 60): t < 30: thin vertical line (ellipse radius 192 × 0.0001 rotated π/2,
  48 segments, width 3); t ≥ 30: f = ((t − 30)/30)⁴, minor axis 192f, width 80f + 3.
- Draw: tint; from T = 70 tint 0x80000030 and a white playfield rectangle, alpha 255 until T = 100 then
  255 − (T − 100)·255/60 until T = 160.

### 2.12 Youmu LS — 人鬼「未来永劫斬」 (0x4123D0 / 0x412FA0), dur 250, invuln 300
Passes isLastSpell = 0 to `Bomb_Start` (QUIRK). Same dash (T < 40) and effect 0x28 (0xFFFF8080) at T = 40.
T = 70: y := 32; centre: C rect life 60, D rect dmg 500 **life 0**, D rect dmg 100 life 60 period 5; effect
0x30 (0xFFFF8080); flash 0xEFFFFFFF; SE 0x2A. Pairs (C rect life 60, D rect life 40, periods 2/3, effect
0x30 each): T = 80 ∓32 dmg 100 (flash 0xCF…), T = 90 ∓64 dmg 100 (0xAF…), T = 100 ∓96 dmg 80 (0x8F…),
T = 110 ∓128 dmg 60 (0x6F…), T = 120 ∓160 dmg 50 (0x5F…), T = 130 ∓192 dmg 40 (0x5F…) and Q := p.
Return dash 150 ≤ T < 180; speed 1 at T = 180. Draw as 2.11 but the rectangle is red (0xFF0000).

### 2.13 Yukari — 境符「四重結界」 (0x410C40 / 0x40D950), dur 150, invuln 200
- At T = 0, 10, 20, 30: C circle(p, r 100, dr 1, life 40); D circle(p, r 100, dr 1, dmg 70, life 40,
  period 5); a boundary effect 0x24 via `Effect_SpawnFixedParam` in fixed slot 4 / 5 / 6 / 7 with param
  (angle, 1, 4), angle = π/4, 3π/8, π/2, 5π/8 (slot 4 at the start position, the others at p); slots 5–7 get
  VM scripts 0x59 / 0x5A / 0x5B (slot 4 keeps 88). SE 0xD at T = 0. Draw: tint only.
- Effect 0x24 init (0x411720) re-spawns its own fixed slot as a type-0x23 ring (angle = param.x, radius
  = param.y, width = param.z) and then sets update 0x4114E0, 44 segments, width 4.
- Update 0x4114E0 (shrinking square): t < 40: k = 1 − t/40; width = 88 − 2t; radius = 192 − 384k²; segments
  −= 1 per frame (44 → 4: the ring becomes a square). At t = 40: flash (3: 8, 1, 0x8FF08080);
  s = radius·0.70711, a = angle + π/4; for each of 4 sides: c = centre + polar(a, s); D rect(c, 8s × 4·width,
  dmg 60, life 70, period 4, rotation a + π/2); C rect(c, 4s × 4·width, item 6, life 100, same rotation);
  a += π/2.

### 2.14 Yukari LS — 境界「永夜四重結界」 (0x410FE0 / 0x4113A0), dur 250, invuln 300
- T = 0: SE 0xD; object 0 pos = p; object-0 VMs scripts 0x15 and 0x16 (player ANM; run every frame; drawn at
  object 0 position + offsets, z 0.01 / 0).
- At T = 0, 10, 20, 30: C circle(p, r 100, dr 1, life 100 / 40 / 100 / 100); D circle as 2.13; effect 0x25 in
  slots 4–7 at the **bomb start position**, same angles; scripts 0x5D / 0x5E / 0x5F for slots 5–7.
- Effect 0x25 init (0x411A80): ring type 0x23, update 0x4117B0, 54 segments, width 6.
- Update 0x4117B0: angle += −π/80 (even slot) / +π/80 (odd) every frame; t < 50: k = 1 − t/50, width =
  88 − 80t/50, radius = (slot − 4)·32 + 192 − ((slot − 4)·32 + 384)·k², segments −= 1; at t = 50: shake
  (1: 16, 8, 0), flash (3: 8, 1, 0x8F6060F0), four side rectangles as 2.13 with D dmg 60 life 100 period 2,
  C life 150.
- Draw: tint 0x802020D0, then the two object-0 VMs (Draw2DRot).

### 2.15 Yuyuko — 死符「ギャストリドリーム」 (0x413140 / 0x413890), dur 300, invuln 350
- T = 0: all 128 objects state 0; 64 butterflies in 4 groups of 16 (i = 0..15): angle = i·π/8 − π,
  centre = p (fixed), radius (+0x1A0) 0; group (VM script, angular speed, radial speed): (0x12, +π/240, 2),
  (0x13, −π/240, 2), (0x12, +π/200, 1.5), (0x13, −π/200, 1.5); each with D circle(r 24, dmg 50, life 500,
  max 800) and C circle(r 24, life 500). Speed multipliers 0.8; effect 20 at p; SE 5; shake (1: 120, 12, 0).
- Every frame: radius += radial speed; angle += angular speed; pos = centre + polar(angle, radius);
  displacement kept; radius < 500: areas follow, VM run (VM end → freed, areas off); radius ≥ 500 → freed.
- Draw: objects 0–95, rotation z = atan2(displacement).

### 2.16 Yuyuko LS — 死蝶「華胥の永眠」 (0x413990 / 0x4142C0), dur 300, invuln 350
As 2.15 (shake (1: 60, 16, 0)) plus: 60 ≤ T < 200, advanced frames with T % 20 == 0: base = Rng_SignedRange(π)
(**RNG**); take up to 16 free objects in index order: VM script 0x14, angle = wrap(index·π/8 + base) with
index = the **object slot** index (QUIRK), angular speed = whatever that slot held (not set — 0 for never
used slots, the old butterfly's value for reused ones; QUIRK), centre = current p, radius 0, radial speed 8;
D circle(r 64, dmg 100, life 500, max 1200); C circle(r 64, life 500). Then SE 0xF (panned at the last
examined object), shake (1: 30, 8, 0), flash (3: 8, 1, 0xE0F0F0F0). Draw: all 128 objects.

### 2.17 Dissolve spell 「ディゾルブスペル」 (0x40D100 / 0x40D310)
`Bomb_Start(dur 120, or 40 in spell practice; invuln 200; 0)`; effect 12 at p (0xFF4040FF); ring effect 0x32 in
fixed slot 4 (0xFF4040FF): 64 segments, radius 8, width 15, 6 waves, VM pos interpolation over 90 frames
(30 in spell practice) mode 5 from (8, 64) to (128, 0) (radius 8 → 128, amplitude 64 → 0); SE 0xD; speed
multipliers 0; flag A bit 6 of all 8 boss slots (0xF54CC0[0..7]) cleared. End handling in 09 §7.

## 3. Options (familiars)
Generic part (09 §5): option = 0x2F4 bytes at player +0x40C + k·0x2F4 (VM, +0x2A4 pos, +0x2B0 anchor,
+0x2BC velocity, +0x2C8 state, +0x2CC sub-state, +0x2D0 index, +0x2D8 orbit angle, +0x2DC facing, +0x2E0
timer, +0x2EC calc fn, +0x2F0 draw fn). State 3 (leaving): interrupt 5 at t = 0, freed (state 0, fns
cleared) at t ≥ 16. Every option's draw fn is 0x44E9E0: VM at pos + shake, z 0.49, Draw2DRot. Options are
drawn by the player draw (prio 9) after the player sprite.
Creation: solos at load; teams: on focus → all 4 slots rewritten from the youkai table (team 3 thereby
drops its option 2 instantly); on unfocus → the youkai options (teams 0–2: all, team 3: slots 0–1) get
state 3 and team 3 slot 2 becomes 0x44F2D0 (trail reset to p).

| fn | used by | behaviour |
|---|---|---|
| 0x44E3A0 | Yukari (shot 0 focused, 5) | §3.1 |
| 0x44EA40 | Alice (shot 1 focused, 7) | init: script 0x1D. Each frame while not bombing: pos = p − (0, 32) (bombs move it, §2.3). Leaving: pos = p − (0, 32) |
| 0x44EB70 | Remilia team ×4 | 09 §5 (anchors fixed at spawn) |
| 0x44EE70 | Remilia solo ×4 | §3.2 |
| 0x44F930 | Yuyuko ×2 (shot 3 focused, 11) | 09 §5 |
| 0x44F2D0 | Youmu's half (team 3 unfocused) | §3.3 |
| 0x44F5E0 | Youmu solo (slot 2) | §3.3 |

### 3.1 Yukari (0x44E3A0)
Init: VM script 0x12; pos = (p.x, max(p.y − 96, 32)); aimed enemy (+0xE2ABC) := 0.
Sub-states 0–2 (following): target = (p.x, max(p.y − 96, 32)); desired = (target − pos)/16; v += (desired −
v)·0.2; pos += v; |v.x| < 0.05 → v.x = 0. If shooting (shot timer ≥ 0), an aimed enemy exists and option
timer ≥ 10 → interrupt 3, sub-state 3; otherwise the aimed enemy is cleared. Animation: sub 0 → v.x > 0:
interrupt 2, sub 2, scale.x negative (mirrored); v.x < 0: interrupt 2, sub 1, scale.x positive; subs 1/2 →
v.x = 0: interrupt 1, sub 0, scale.x positive; v.x of the other sign: interrupt 2 and switch.
Sub 3 (attack): while an aimed enemy exists: same smoothing toward enemy pos (+0x2D88) + (0, 32) (y ≥ 32);
leaves to sub 0 (interrupt 1, aimed := 0) when not shooting and the shot button is released, or no enemy.

### 3.2 Remilia solo (0x44EE70)
Init: script 0x18; anchor = p + offset, offsets by index: 0 (−30, −16), 1 (−10, −32), 2 (+10, −32),
3 (+30, −16); orbit angle 0, π, 0, π. Each frame: option timer ≥ 12 → orbit angle += +π/120, −π/90, +π/90,
−π/120 (by index). Colour 0xFFFF8080; when the player is unfocused (+3 = 0): colour 0xFF80FFFF and
anchor += (p + offset − anchor)·0.2 (focused: anchor frozen). pos = anchor + polar(orbit, 8), z 0; effect 0x2F
(0x80602050) at the option every frame.

### 3.3 Youmu's half (0x44F2D0) and Youmu solo (0x44F5E0)
Init: script 0x15; anchor = trail[15] (player +0x380, the oldest of the 16 stored positions); orbit 0;
facing (+0x2DC) = −π/2. Each frame: orbit += π/60; anchor += (trail[15] − anchor)·0.05; pos = anchor +
polar(orbit, 8), z 0. Facing target from the direction code (09 §3): 1 up +π/2, 2 down −π/2, 3 left 0,
4 right π, 5 up-left π/4, 6 up-right 3π/4, 7 down-left −π/4, 8 down-right −3π/4 (i.e. pointing away from the
motion); code 0 → unchanged. If |facing − target| > π, target is shifted by 2π toward facing; if the
remaining difference ≤ π/2 → facing = wrap(facing + (target − facing)·0.07), else facing = target.
Team version: effect 0x2F (0x80405080) every frame. Solo version: unfocused → colour white, facing tracks,
effect 0x80405080; focused → colour 0xFFFF8080, facing frozen, effect 0xFFF05080.
Shot init 8 (`option angle`) fires along option 2's facing (player +0xCD0 = option 2 +0x2DC).

## 4. Player laser (Alice's focused shot, `ply01as.sht`, the only data using lasers)
Entries (one per power level): interval 2/4/6/8/12/16 (also used as trail length n), phase 0 (= laser
slot 0), origin option 0, hitbox 24 × 18, angle −π/2, speed 10, damage 7, type 5, script 14–18 (+10),
init 4, update 5, draw "trail", hit filter 2. Laser slots: **3** × {Timer, bullet ptr} at player +0xE2A38
(constructed with count 3) plus the entry per slot at +0xE2A80 (09 said 4 — corrected).
- Init (0x44FE20): not while bombing nor with 0x164D0B4 bit 13. Free slot → slot timer 999, owner = this
  bullet; bullet +0x466 slot, +0x468 origin, +0x444 offset, +0x46A = interval; default spawn; trail (32 × f32[3]
  at bullet +0x2B0) x = −999; pos.x = −999; slot entry = entry; slot taken. Slot owned by a different entry
  (power level changed) → the old bullet's VM gets interrupt 1 and the slot is cleared (no new laser this
  frame). Same entry → nothing.
- Update 5 (0x450840), every frame: if the bullet no longer owns the slot, or a dialogue runs, or the shot
  timer < 0 (not shooting), or player state 2, or bombing, or bit 13: interrupt 1, slot cleared, update fn
  removed (the bullet then just flies with its velocity, is not culled and is freed when its VM ends).
  If option 0 is gone (state 0) → slot cleared, bullet freed. Otherwise: for i < n where trail[2i].x ≥ −900:
  one-frame D rect(trail[2i], 16 × 448, dmg 1, no hit effect); shift the trail (trail[i] = trail[i − 1] and
  y −= 1, i = 31..1); trail[0] = pos; pos = option-0 pos, z 0.44; hitbox height 448; pos.y −= 208; colour
  (R 0xFF, G 0xD0, B 0xB0) at youkai-max gauge else white; then the generic bullet step adds velocity (y −10),
  runs the VM and ticks the timer. The bullet itself hits enemies only on even bullet-timer frames (09 §4.5).
  End condition: release shot / dialogue / bomb / death / option lost / level change (above).
- Trail draw (0x450AD0): a0 = VM alpha·3/4; for c = 0, 2, 4, … < 2n while trail[c].x ≠ −999: draw the VM at
  trail[c] + shake with alpha = original (c = 0) or a0 − (a0/2)·c/n; at youkai max colour (0xFF, 0x40, 0x40);
  alpha restored afterwards.
- Update 4 (0x4505D0) is not referenced by any shipped entry (a type-4 laser: lifetime from the slot timer,
  capped at 20 during dialogue/bomb, freed at 0; interrupt 1 when ≤ 70 and VM flag bit 14; scale.y = pos.y/14,
  hitbox height = pos.y, pos.y halved).

## 5. Shot drawing, red tint, hitbox marker
- Player draw prio 9 (0x44D530): (1) flying bullets (state 1, 0x4512F0): if the VM tag (+0x1FC, set by
  ANM op 25) ≠ 0 → VM rotation z = bullet angle (+0x450) (exactly the movement angle; no π/2 offset);
  pos + shake, z 0.4; red tint flag (+0x470) → VM colour RGB = (0xFF, 0x40, 0x40) (alpha kept); Draw2DRot;
  then the entry's extra draw fn (trail). (2) Bomb draw fn while bombing. (3) Player sprite at p + shake,
  z 0.1, Draw2DSnap (hidden while the continue menu is up). (4) Option draw fns.
- Player draw prio 10 (0x44D630 → 0x451400): bullets in their hit animation (state 2): same rotation/tint
  rule, z 0.2, drawn with the VM's own draw mode.
- Red tint flag: set at spawn when entry side > 0 and gauge ≥ youkai max (09 §4.3).
- Focused hitbox marker: on every switch to focused (all shot types) the player creates effect 22 in fixed
  slot 2 (if none, kept in player +0xBE834): `etama.anm` script 54 = sprite 218 (64 × 64 at (0, 112) of
  etama2.png, the red ring), fade-in to alpha 255 over 20 frames, spin π/120 per frame, follows the player;
  on unfocus it receives interrupt 1 → fades out over 30 frames and is freed. It is drawn in the effect pass
  (prio 12, alpha list, z 0.07), i.e. above enemies and below bullets.

## 6. Corrections to 09
- 09 §1.1 / §4.1 "+0xE2A38 4 × 0x10 laser slots" → 3 slots.
- 09 §7 bomb table: Yukari's bomb draw is the tint-only 0x40D950; Youmu LS passes isLastSpell = 0; Reimu
  LS cancel at the orbs' explosions is centred on the player; the full choreography above supersedes the
  "INFERRED detail level" summary.
