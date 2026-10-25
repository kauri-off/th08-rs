# 14 — Bomb choreography, options, player laser, shot drawing

Sources: bomb table 0x4C7AD0 (09 §7), `Bomb_Start` 0x40BE30, `Player_SpawnInvulnRing` 0x40BF00, bomb tint
0x40BC60, per-bomb calc/draw functions (below), effect updates 0x40E040–0x40E2D0 / 0x410BB0 / 0x4114E0 /
0x4117B0 / 0x413070, option functions (tables 0x4C7D40 / 0x4C7E10), laser 0x44FE20 / 0x450840 / 0x450AD0,
player draw 0x44D530 / 0x44D630 (bullets 0x4512F0 / 0x451400). VERIFIED unless marked.
Positions written to VMs are playfield coordinates plus the playfield origin (globals 0x164D2DC/0x164D2E0 = 32, 16); the
screen-shake offset (AnmManager +0x1C/+0x20) is added later by the draw routines (05 §9). Screen effects are written (type: p0, p1, p2[, p3]) in `ScreenEffect_Register` order: type in ECX, **p0 (duration) in EDX**, p1–p3
on the stack, then the draw prio (0x15 everywhere here); a decompiler shows only the stack arguments, so p0 must be read from
the `mov edx` before each call (VERIFIED for every call site, RE-Q29: Alice shakes are 24 frames, Sakuya 120 / Sakuya LS 50).
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
Spell-name banner (`Gui_ShowPlayerSpellName`, SE 0xE; EDX = cut-in face 0 human / 1 youkai / −1 dissolve; isLastSpell
only selects the cdbg strip sprite, and Youmu LS passes 0 — QUIRK; 12 §5.8); player +0xFE4 = duration; player state 3 with
invulnerability timer; invulnerability ring (effect 23, fixed slot 0, 13 §2.6); all bomb objects state 0; all
items attracted; bomb origin (+0xB8828) = p. (09 §7 for the surrounding start/end logic.)

### 1.3 Background tint (0x40BC60, called first by every bomb draw function, EDX = colour C)
Per channel: T < 60 → 128 + (C − 128)·T/60; T ≥ duration − 60 → 128 + (C − 128)·(duration − T)/60;
else C. Alpha 0x80. It first turns the global tint off (0x40BAB0), then submits the result as the stage tint
request (`Stage_RequestTint` 0x409160) and sets the stage flag +0x646C (0x4EA49C) = "a bomb drew this frame",
which stops the next `Stage_DrawHigh` from switching the tint off: the tint applied by the next frame's
`Stage_DrawLow` therefore also covers the background effects, spell background, prio-8 enemies and the
flying player bullets, not only the stage layers (06 §10.2, changelog/answers_impl11.md §1, VERIFIED).
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

Float constants in this section are exact f32 values unless written as hex: the growth rates 64/15 and 128/15
are the exe's 0x40888889 (4.2666669) and 0x41088889 (8.5333338), not the decimals 4.267 / 8.533 (which are other
floats and change the radii after a few frames; RE-Q52).

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
  both areas off; C circle(**p**, r 64, dr 64/15 (f32 0x40888889), life 30); D circle(orb, r 64, dr 12.8, dmg 500, life 12,
  period 4, unlimited); effect 6 ×8; state 2; VM interrupt 1; SE 0xF @orb; shake (1: 16, 8, 0). (The code
  computes velocity/8 into a temporary and discards it — the orb keeps its speed. QUIRK)
- T ≥ 40 every frame (state ≠ 0): pos += velocity·gs.
- Every frame, any T (also during the spin-out T < 40), after the movement: state 1 → both attached areas (C r 96
  and D r 64) are moved to the orb's current position (so C does not stay at p); state 2: counter += 1 on
  advanced frames, freed after 30; VM run (changelog/answers_impl11.md §3, VERIFIED).
- Hit effect: only the 16 D r 64 orb circles have +0x3D ("no hit effect") set; the impact circles (dmg 500)
  produce the normal spark (changelog/answers_impl11.md §2, VERIFIED).
- Draw: orbs 0–15 (Draw2DSnap at pos + offset + playfield origin, z 0).

### 2.2 Reimu LS — 神霊「夢想封印　瞬」 (0x40C910 / 0x40D010), dur 200, invuln 260
- T = 0: effect 12 at p (0xFF4040FF) and the same 16 orbs and areas as 2.1 (D orb circles with +0x3D);
  blast index (+0xFF0) = 0; SE 0xD. The impact circles (dmg 25) and the blasts (dmg 400) do **not** set +0x3D
  (changelog/answers_impl11.md §2/§4, VERIFIED).
- Every frame, state-1 orbs: angle ±π/60 as above, pos = centre + (sin, cos)·radius; while T < 40:
  radius += 2.4 (even) / 1.2 (odd). Orb i explodes when T ≥ duration − 40 − i: areas off; C circle(p, r 64,
  dr 64/15 (f32 0x40888889), life 30); D circle(orb, r 64, dr 128/15 (f32 0x41088889), dmg 25, life 15, period 5, max 50); effect 6 ×8; state 2
  (not freed; its VM keeps running); interrupt 1; SE 0xF @orb; shake (1: 16, 8, 0). The new circles are **not**
  attached to the orb (its +0x16EC/+0x16E8 pointers keep the T = 0 areas), but the orb's area-follow write
  (C and D pos := orb pos) still runs **after** the impact in the same frame, through those old pointers. The old
  slots were just switched off, so a new circle that took the first inactive slot of its half (09 §8.1) sits in
  exactly that slot: normally the impact C circle is moved from p to the orb position (and the D circle written
  with the position it already has). If a lower slot was free, the write goes to an inactive slot and the C
  circle stays at p. Later frames (state 2) write nothing. QUIRK, VERIFIED 0x40C910 + Dn14 pilot run
  (re/work/runs/q52/orig_Dn14, stage 6A frames 5049–5072: cancel areas 192–201 centred on the orbs, growing
  64 → 128; a port with C at p cancels bullets 985 / 1064 frames later, RE-Q52). 2.1 has no such write: its follow
  step only handles state-1 orbs, after the impact loop.
- Blasts: when T ≥ 40 and T % 20 == 0 (T = 40, 60, …, 180): object 16 + (+0xFF0), then +0xFF0 := 1 (so the
  first blast uses object 16 and every later one reuses object 17 — QUIRK); VM script 0x14 (`AnmFile_SetScript` on the player ANM: runs its time-0 instructions now, before the point is drawn; no player*.anm script draws RNG, so the order changes no RNG draw; VERIFIED 0x40C910, RE-Q24); colour =
  table 0x4C6100[(T/20) % 7] = {0x8FFFFFFF, 0x8F0000FF, 0x8FFF00FF, 0x8FFF0000, 0x8FFFFF00, 0x8F00FF00,
  0x8F00FFFF}; point = homing target if x > −100 else (R·320 + 32, R·384 + 32) (**RNG**, x first);
  effects 0x31 and 0x37 (colour); C circle(point, r 64, dr 64/15 (f32 0x40888889), life 30); D circle(point, r 64, dr 128/15 (f32 0x41088889),
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
  (1: 24, 8, 0); flash (3: 8, 1, 0x8FFFFFFF). T = 100, 110, 120, 130: effect 0x2D colours 0xFFFFFFFF,
  0xFFFFD0D0, 0xFFFF8080, 0xFFFF0000. T = 150: flash (3: 8, 1, 0x8FFFFFFF), shake (1: 24, 8, 0), SE 0x19.
  T = 209: option state := 1, option timer 0 (the doll re-initialises).
- Ring effects 0x2A–0x2D (k = 1 − (1 − t/40)², so they expand for 40 frames then collapse; life 80): all
  spawned with colour 0xFFFFFFFF (also in 2.4, except the four coloured 0x2D); script 83 (etama sprite 224, normal
  blend) sets alpha 0 → 255 over 8 frames, holds, alpha → 0 from t = 50 over 30, `hide_stop` at 80 (freed);
  0x2A wavy ring radius 256k, 64 segments, 5 waves, width 8; for t < 40 the amplitude (+0x32C) is never
  written and stays 0 from the ring init → a **plain** ring; t ≥ 40: amplitude 64k, width +2/frame;
  0x2B / 0x2C ellipses (semi-axes 256k × 128k) rotated +π/4 / −π/4, 48 segments, width 8 (t ≥ 40: +1.5/frame);
  0x2D octagon (8 segments) radius 192k, width 8.

### 2.4 Alice LS — 魔操「リターンイナニメトネス」 (0x40D970 / 0x40DEE0), dur 230, invuln 280
As 2.3 but: start S = **p** (no SE 6); doll alpha 0 from T ≥ 128; events: T = 60–76 effect 0x28 as above;
T = 120: SE 0xF, effects 0x2A–0x2C at the doll and effect 0x2D at (64, 96) 0xFF0000F0, (64, 352) 0xFFF00000,
(320, 352) 0xFF00F000, (320, 96) 0xFF00F0F0; C circle(doll, r 1, dr 5, life 110); D circle(doll, r 1, dr 5,
dmg 70, life 110, period 5) (no shake/flash); T = 130, 140, 150, 160: effect 0x2D colours as 2.3; T = 180:
flash (3: 8, 1, 0xFFFFFFFF), shake (1: 24, 8, 0), SE 0x19; T = 229: doll re-initialises.
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
  pos = p + (cos a, sin a)·(sprite width·scale.x/2); rotation z = a; Draw2DRot (+ playfield origin, z 0).

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
- 20 ≤ T < 116, every frame: SE 6 and shake (1: 120, 4, 1). For each knife k in index order: at T =
  (k % 48)·2 + 20: if the knife is already active the whole function **returns** for this frame (QUIRK);
  else activate: VM script 0x16; angle = k·2π/96 − π; speed (+0xC) = R + 0.5; accel (+8) = R·0.1 + 0.03;
  spin = (Rng_Range16(1) == 0 ? −π/20 : +π/20) — always −π/20 (QUIRK; one RNG draw) (**3 RNG calls per knife**);
  pos = p + polar(angle, 24); age 0; C circle(knife, r 32, life 500); D circle(knife, r 32, dmg 20, life 500).
  Knives k and k + 48 appear together, two every second frame.
- Each active knife: age < 30 or age > 69: at age = 70 → if the homing target exists (x > −100) angle =
  atan2(target − pos); speed := 14. speed += accel; velocity = polar(angle, speed). 30 ≤ age ≤ 69: angle +=
  spin, velocity 0 (hover and spin). Only while the knife still holds its D-area pointer: areas follow;
  age < 120: if its D area has dealt damage (+0x30 > 0) → VM script 0x17, effect 0 (0xFFFF80FF), both areas
  off **and both pointers cleared**, SE 0x2B @knife; age ≥ 120: areas off, pointers cleared. The hit branch
  therefore runs at most once per knife (changelog/answers_impl11.md §5, VERIFIED). pos += velocity (not
  scaled by gs); VM run; age ticks. Knives are never freed before the next bomb.
- Draw: active knives 0–95, rotation z = angle, Draw2DRot.

### 2.8 Sakuya LS — 幻葬「夜霧の幻影殺人鬼」 (0x4103F0 / 0x410AC0), dur 320, invuln 350
As 2.7 but 128 knives; shake (1: 50, 4, 1) only once at T = 0 (SE 6 is still played every window frame that
does not return early; effect 20 at p is kept and follows p while T ≤ 60 as in 2.7); window 20 ≤ T < 148; knife k at
T = (k % 64)·2 + 20 with angle = wrap(k·2π/64 − π) and VM script 0x14; damage 30; at age 70 additionally
effect 0x2E (ring: radius +8/frame, 12 segments, width 32; script 84: alpha 255, from t = 8 → 0 over 7,
`hide_stop` at 23 → freed after 23 frames; the update never frees it) at the knife; on hit: every knife with k % 3 == 0
also flashes (3: 2, 1, 0x208080FF); hit script 0x15. Draw function only draws knives 0–95 (knives 96–127
are invisible but still damage — QUIRK).

### 2.9 Remilia — 紅符「不夜城レッド」 (0x40EE10 / 0x40F550), dur 240, invuln 290
The bats are the 4 options; their anchors (option +0x2B0 = player +0x6BC + k·0x2F4) are moved:
- T = 0: SE 0xD; start anchors saved (objects 0–3); SE 6; counter 0; speed multipliers 0.
- T < 60: anchor_k = S_k + (A_k − S_k)·(T/60)² with A0 = (p.x − 32, p.y), A1 = (p.x, p.y − 32),
  A2 = (p.x, p.y + 32), A3 = (p.x + 32, p.y).
- T ≥ 60: at T = 60 speed multipliers 2 and interrupt 2 to the four option VMs; anchors := A_k every frame;
  C circle(p, r 96, life 0). Advanced frames with T % 10 == 0: ring effect 0x35 in slot 4 + (n % 4), 32
  segments, 4 waves; pos interpolation 30 frames mode 4 from (0, 0, 0) to (192, R·128, 0) (radius 0 → 192,
  wave amplitude 0 → random, **RNG**); scale interpolation 30 frames mode 1 (64, 0) → (64, 0) (width constant
  64); alpha 255 → 0 mode 3; colour white → 0xFFFF0000 mode 0; VM run once; n += 1; SE 0x11 @p. Always the
  type's own script (88): no odd-n script switch, unlike Marisa LS (changelog/answers_impl11.md §8, VERIFIED).
  While the shot timer ≥ 5 (shooting): C rect(p, 96 × 800) and C rect(p, 800 × 96), life 0 (a cross).
  T = 239: the 4 options re-initialise (state 1, timers 0).
- No damage areas: the damage comes from the shot table — `ply02as.sht` levels 6/7 are used while a
  Remilia bomb has run ≥ 60 frames (09 §2.1).

### 2.10 Remilia LS — 紅魔「スカーレットデビル」 (0x40F570 / 0x40FCB0), dur 280, invuln 320
Identical to 2.9 except speed multipliers 3 at T = 60, ring scale interpolation mode 1 (64, 0) → (128, 0) over
30 frames (width 64 → 128; same start pos (0, 0, 0), no script switch), options
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
  = param.y, width = param.z) and then sets update 0x4114E0, 44 segments, width 4. The re-spawn leaves the slot
  with type 0x23 and **script 88** (type 0x23's); slots 5–7 then get 0x59–0x5B from the bomb.
- Lifetime: the update always returns 1 (never frees); the effect is freed when its VM reaches `hide_stop`:
  scripts 88–91 (etama entry 2 scripts 12–15): additive, alpha 0 → 255 over 20, at t = 80 → 0 over 60, hidden
  and freed at **t = 140** (the side rectangles keep their own area lifetimes) (changelog/answers_impl11.md §11, VERIFIED).
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
- Effect 0x25 init (0x411A80): ring type 0x23, update 0x4117B0, 54 segments, width 6. As in 2.13 the re-spawn
  turns slot 4 into script **88** (life 140, QUIRK: not 0x5C) while slots 5–7 get 0x5D–0x5F (etama entry 2
  scripts 17–19: alpha 0 → 255 over 20, at t = 160 → 0 over 60, freed at **t = 220**). The update never frees
  (changelog/answers_impl11.md §11, VERIFIED).
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
Trigger (changelog/answers_impl11.md §14, VERIFIED): bomb index 4 is chosen at bomb start whenever GameFlags (0x164D0B4) bits
7–8 ≠ 0, i.e. while a boss **last spell** runs (ECL op 176 sets bit 7) or after one was captured (bit 8, kept
until the next op 176 / this spell's end / stage start). In that state a bomb press only plays SE 0x29 (also in
spell practice, bit 14), so the dissolve spell starts only as the **forced bomb** of a hit (`Player_Die`: special
mode → counter 2, +6 = 1). Spell practice of a last-spell card gives the 40/30-frame variant. `Bomb_Start` is
called with EDX = −1, so no player cut-in is (re)started and the cdbg strips are not drawn (12 §5.8); the card
name still appears. End (`Player_UpdateBomb`, index 4): GameFlags bits 7–8 cleared; for each boss slot:
`Enemy_KillChildren(boss, 0)`, boss life (+0x2DFC) := 0, flag A bit 30 cleared; flash (type 3: 30 frames,
1 repeat, colour **0xFFFFFFFF**).

## 3. Options (familiars)
Generic part (09 §5): option = 0x2F4 bytes at player +0x40C + k·0x2F4 (VM, +0x2A4 pos, +0x2B0 anchor,
+0x2BC velocity, +0x2C8 state, +0x2CC sub-state, +0x2D0 index, +0x2D8 orbit angle, +0x2DC facing, +0x2E0
timer, +0x2EC calc fn, +0x2F0 draw fn). State 3 (leaving; set by `Player_Move` on unfocus with the option timer +0x2E0 Set(0)): the calc fn only
sends interrupt 5 when t == 0 and frees the option (state 0, calc/draw fns cleared) when **t > 16** — no motion,
no effect 0x2F, no colour/facing work (VERIFIED for all 7 calc fns; changelog/answers_impl14.md §5). Exception: Alice
(0x44EA40) keeps pos = player pos − (0, 32) every leaving frame (even while bombing). The generic loop in
`Player_Move` still runs `AnmVm_Run` and ticks the option timer after the calc fn every frame, including the
freeing frame; nothing is drawn once the draw fn is cleared. Every option's draw fn is 0x44E9E0: VM at pos + playfield origin, z 0.49, Draw2DRot. Options are
drawn by the player draw (prio 9) after the player sprite.
Creation: solos at load; teams: on focus → all 4 slots rewritten from the youkai table (team 3 thereby
drops its option 2 instantly); on unfocus → the youkai options (teams 0–2: all, team 3: slots 0–1) get
state 3 and team 3 slot 2 becomes 0x44F2D0 (trail reset to p).

| fn | used by | behaviour |
|---|---|---|
| 0x44E3A0 | Yukari (shot 0 focused, 5) | §3.1 |
| 0x44EA40 | Alice (shot 1 focused, 7) | init: script 0x1D. Each frame while not bombing: pos = p − (0, 32) (bombs move it, §2.3). Leaving: pos = p − (0, 32) |
| 0x44EB70 | Remilia team ×4 | init: script 0x18; anchor = p + offset with the solo offsets (0 (−30, −16), 1 (−10, −32), 2 (+10, −32), 3 (+30, −16)), then **fixed**; orbit angle 0, π, 0, π. Each frame: option timer (+0x2E0 current) **> 12** → orbit += +π/120, −π/90, +π/90, −π/120 (by index); pos = anchor + polar(orbit, 8). The timer is ticked after the calc fn and the VM run (09 §5) and the init call (state 1 → 2) also runs the orbit step with t = 0, so the first rotation happens on the 14th calc of the option (t = 13); SPEC FIXED RE-Q27 (`Timer_Greater`(t, 12) 0x44EB70; was "≥ 12"; attract demo 3: the original's options rotate from frame 1590, one frame after the old rule). No colour change, **no effect 0x2F** (changelog/answers_impl11.md §9, VERIFIED) |
| 0x44EE70 | Remilia solo ×4 | §3.2 |
| 0x44F930 | Yuyuko ×2 (shot 3 focused, 11) | init: script 0x15; anchor = p; orbit 0 (index 0) / −π (index 1). Each frame: target = p − (32, 0) (index 0, orbit += π/60) or p + (32, 0) (index 1, orbit −= π/60); anchor += (target − anchor)·0.09; pos = anchor + polar(orbit, 6), z 0; effect 0x2F at pos, colour **0x80602050**, every frame (changelog/answers_impl11.md §9, VERIFIED) |
| 0x44F2D0 | Youmu's half (team 3 unfocused) | §3.3 |
| 0x44F5E0 | Youmu solo (slot 2) | §3.3 |

### 3.1 Yukari (0x44E3A0)
Init: VM script 0x12; pos = (p.x, max(p.y − 96, 32)); aimed enemy (+0xE2ABC) := 0.
Sub-states 0–2 (following): target = (p.x, max(p.y − 96, 32)); desired = (target − pos)/16; v += (desired −
v)·0.2; pos += v; |v.x| < 0.05 → v.x = 0. If shooting (shot timer ≥ 0), an aimed enemy exists and option
timer ≥ 10 → interrupt 3, sub-state 3 (aimed kept); otherwise the aimed enemy is cleared. **Branch order**
(`Option_YukariFollow` 0x44E770 does the follow step and this attack test; the caller then runs the animation
test below **unconditionally**, using the sub-state it entered with): the animation test can overwrite the
sub-state 3 just set (and its interrupt 3 with interrupt 2/1), while the aimed pointer stays set, so the attack
starts on the next frame (golden 1462–1464, changelog/answers_impl21.md §Q2). Animation: sub 0 → v.x > 0:
interrupt 2, sub 2, scale.x negative (mirrored); v.x < 0: interrupt 2, sub 1, scale.x positive; subs 1/2 →
v.x = 0: interrupt 1, sub 0, scale.x positive; v.x of the other sign: interrupt 2 and switch.
Sub 3 (attack): while an aimed enemy exists: same smoothing toward enemy pos (+0x2D88) + (0, 32) (y ≥ 32);
leaves to sub 0 (interrupt 1, aimed := 0) when not shooting and the shot button is released, or no enemy.
Aimed enemy (player +0xE2ABC = 0x18B89B4, VERIFIED, changelog/answers_impl11.md §12): a persistent pointer, never re-picked
from scratch. Each frame `EnemyMgr_Calc`, for every live enemy with flag A bits 4, 5, 11 clear and (bit 31 clear
or not bombing), replaces it when |enemy final x − p.x| < 64 and the enemy has no parent (+0x2DA4 = 0) and
(pointer = 0 or enemy **final** y (+0x2D8C) < aimed enemy's **base** y (+0x2D38); mixed fields, QUIRK). It is
cleared only when the aimed enemy's slot is found inactive or freed (`Enemy_Free`), or by the option itself
(sub-states 0–2 when not attacking, leaving sub 3). So in sub-state 3 Yukari keeps chasing the same enemy
after it leaves the 64-px column, unless a higher enemy enters the column.
Ordering (changelog/answers_impl18.md §Q1, VERIFIED at runtime, golden frames 1431–1433): the pointer is updated in the enemy's
damage step **before** its death check, and death mode 0 does not clear it, so an enemy killed in the same frame
— golden frame 1431: slot 7 spawned by the timeline at (284.0, −21.7), killed by shots in its first update,
life 0 — is left as the aimed enemy after that frame. The player calc (prio 9) runs before the enemies (prio 11):
on the next frame the option sees the pointer, enters sub-state 3 (interrupt 3) and makes its first attack move
toward that dead enemy's final pos + (0, 32) (y ≥ 32); then that frame's `EnemyMgr_Calc` finds slot 7 inactive
and clears the pointer, and the frame after the option leaves sub-state 3 (interrupt 1, sub 0).

### 3.2 Remilia solo (0x44EE70)
Init: script 0x18; anchor = p + offset, offsets by index: 0 (−30, −16), 1 (−10, −32), 2 (+10, −32),
3 (+30, −16); orbit angle 0, π, 0, π. Each frame: option timer **> 12** (as the team version, §3 table; SPEC FIXED RE-Q27) → orbit angle += +π/120, −π/90, +π/90,
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
Team version: effect 0x2F (0x80405080) every frame, spawned **before** the facing step, so also on direction code 0.
Solo version: colour 0xFFFF8080 is written first; unfocused → colour white, then the facing step, then effect 0x80405080
— but on direction code **0** the function returns right after the white colour write (jump table 0x44F90A[0] →
0x44F76A → epilogue 0x44F904, skipping both `Effect_Spawn` calls 0x44F885 / 0x44F8A4): **no effect 0x2F and no RNG**
while the unfocused solo Youmu stands still; focused → colour 0xFFFF8080, facing frozen, effect 0xFFF05080 every frame
(any direction). (VERIFIED 0x44F5E0 / 0x44F2D0, RE-Q49; silentselene 51852 stage 1: no draws on frames 1, 3, 4.)
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
  **"Different entry" is a pointer comparison** (slot entry +0xE2A80[slot] ≠ the entry being fired), not a content
  comparison (VERIFIED 0x44FE20, RE-Q50). `Player_Load` loads the human and the youkai sht with two separate
  `Sht_Load` calls into +0xE2A74 / +0xE2A78, each `FileSystem_OpenFile` call decoding into its own buffer, so even
  when both are the same file (solo shot types, 09 §2: solo Alice = shot 7 uses `ply01as.sht` for both) the
  focused and unfocused entries are different objects. A **focus change therefore ends the laser** like a level
  change: on the first firing frame with the other sht the old laser gets interrupt 1 and the slot is cleared (no
  bullet this frame); on the next firing frame (entry interval 2 at power < 8, …) a new laser is created with
  bullet timer 0. The new timer phase decides the even-frame hits of 09 §4.5 (data: silentselene 52104, Alice solo,
  focus released at stage-1 frame 403 → new laser, timer 0 at frame 404, hits at 432/434).
- Update 5 (0x450840), every frame: if the bullet no longer owns the slot, or a dialogue runs, or the shot
  timer < 0 (not shooting), or player state 2, or bombing, or bit 13: interrupt 1, slot cleared, update fn
  removed (from the next frame on the bullet just flies with its velocity, is not culled and is freed when its VM
  ends). **QUIRK: this branch does not return** (VERIFIED 0x450840, RE-Q51): it falls through into the rest of the
  update, so on its last frame the laser still adds its trail rects, shifts the trail and moves to option 0, and is
  kept. Example (silentselene 52104, frame 490): the laser ended by the frame-489 focus change still follows the
  player to x 82.6 in frame 490 and keeps hitting from there.
  Then (every frame, including that last one): if option 0 is gone (state 0) → slot cleared, bullet freed. Otherwise: for i < n where trail[2i].x ≥ −900:
  one-frame D rect(trail[2i], 16 × 448, dmg 1, no hit effect); shift the trail (trail[i] = trail[i − 1] and
  y −= 1, i = 31..1); trail[0] = pos; pos = option-0 pos, z 0.44; hitbox height 448; pos.y −= 208; colour
  (R 0xFF, G 0xD0, B 0xB0) at youkai-max gauge else white; then the generic bullet step adds velocity (y −10),
  runs the VM and ticks the timer. The bullet itself hits enemies only on even bullet-timer frames (09 §4.5).
  End condition: release shot / dialogue / bomb / death / option lost / level change / focus change (above).
- Trail draw (0x450AD0): a0 = VM alpha·3/4; for c = 0, 2, 4, … < 2n while trail[c].x ≠ −999: draw the VM at
  trail[c] + playfield origin with alpha = original (c = 0) or a0 − (a0/2)·c/n; at youkai max colour (0xFF, 0x40, 0x40);
  alpha restored afterwards.
- Update 4 (0x4505D0) is not referenced by any shipped entry (a type-4 laser: lifetime from the slot timer,
  capped at 20 during dialogue/bomb, freed at 0; interrupt 1 when ≤ 70 and VM flag bit 14; scale.y = pos.y/14,
  hitbox height = pos.y, pos.y halved).

## 5. Shot drawing, red tint, hitbox marker
- Player draw prio 9 (0x44D530): (1) flying bullets (state 1, 0x4512F0): if the VM tag (+0x1FC, set by
  ANM op 25) ≠ 0 → VM rotation z = bullet angle (+0x450) (exactly the movement angle; no π/2 offset);
  pos + playfield origin, z 0.4; red tint flag (+0x470) → VM colour RGB = (0xFF, 0x40, 0x40) (alpha kept); Draw2DRot;
  then the entry's extra draw fn (trail). (2) Bomb draw fn while bombing. (3) Player sprite at p + playfield origin,
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
