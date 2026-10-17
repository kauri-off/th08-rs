# Answers to IMPL9_QUESTIONS.md

All addresses are from th08.exe. Function names are from `symbols.csv` (fresh dump, `tools/redump.sh`).
VERIFIED = read from code; INFERRED = reasoning, not read directly.

---

## Priority 1 — Hit spark (effect type 3) — VERIFIED

**A player bullet hitting an enemy never spawns type 3.** Type 3 is spawned in exactly one place in the
binary: the damage-area loop of `Player_DamageAgainst` 0x451670 (the only `Effect_Spawn(3, …)` call, near
0x451BF0). Bullets use different code; see the end of this section.

### Spawn condition

`Player_DamageAgainst` is called by `EnemyMgr_Calc` once per enemy per frame with that enemy's hitbox. It
loops over the 192 player damage areas (player +0xB8834, 0x40 each, 09 §8). These are areas created by
bombs and last spells. For each area:

1. The area must be active (+0x3C ≠ 0).
2. The area's life must be a multiple of its period: `life % period == 0`.
3. The area must overlap the enemy box (circle, rectangle, or rotated rectangle).

When all three hold, the area's damage is added. Then the spark is considered:

```
if area.no_hit_effect (+0x3D) == 0:
    player.spark_counter (+0xE2A94, u8) += 1
    if (spark_counter & 3) == 0:
        Effect_Spawn(type 3, pos = enemy box centre, count 1, colour 0xFFFFFFFF)
```

- The `+0x3D` "no hit effect" byte is set by several bombs: 0x40C010 Reimu, 0x40C910 ReimuLS,
  0x40E3B0 Marisa, 0x40E780 MarisaLS, and 0x450840. Those areas never spark.
- The counter is shared with `PlayerBullet_HitSpark8` 0x450EE0 (below).
- The counter is never reset. It is a wrapping u8 that persists across frames, enemies and areas.
- The code has a test `index < 0xC0 ? type 3 : type 5`. It is always true inside the loop, so the
  spark is always type 3.
- There is no boss exclusion and no damage threshold. The only limits are the 1-in-4 rate and the area
  period.

### Position, colour, count

- **Position:** the `pos` argument of `Player_DamageAgainst`, i.e. the enemy hitbox centre in playfield
  coordinates. The effect is drawn at pos + playfield origin.
- **Colour:** 0xFFFFFFFF.
- **Count:** 1.

### Script and motion

The script is etama.anm global script 31 (`anm_dis.txt`, entry 1 script 6):
- sprite 195, z-write off, **additive** blend;
- alpha fades 255 → 0 over 40 frames;
- scale 3.0, growing by −0.075 per frame (reaches 0 at frame 40);
- hide and stop at frame 40.

### RNG (2 draws, replay-relevant)

`EffInit_Spark3` 0x425EA0:
- vx = (Rng_FloatRange(256) − 128) · 4 / 33, then vy the same way (x draw first);
- vz = 0;
- acceleration = −v / 20;
- both v and a are multiplied by game speed.

`EffUpd_VelAccel` 0x425E60 runs each frame: pos += v, then v += a.

### Effects bullets do produce

| function | effect |
|---|---|
| bullet enters its hit state (state 1 → 2, not types 4/5/6) | type 5 at the bullet |
| explosion hit filter 0x450C50 (09 §4.3, "hit" row) | type 5 at the enemy when the bullet timer % 6 == 0 |
| `PlayerBullet_HitSpark8` 0x450EE0 (a bullet hit filter) | spark counter += 1; type 5 at (bullet x, enemy y, enemy z) when `(counter & 7) == 0` |

**Port fix:**
- Remove the per-frame type-3 spawn from the shot-hit block.
- Spawn type 3 only from bomb/last-spell damage areas, with the rule above.
- The 2 RNG draws then happen only on those sparks, which matters for replay sync.

---

## Priority 2 — Ring texture mapping (types 35/39/49/52/59 and every ring) — VERIFIED

### Functions

| function | role |
|---|---|
| `EffDraw_Ring` 0x427450 | ring custom draw: builds positions when +0x356 (dirty) is set, then draws |
| `Anm_RingSetUV` 0x464B00 | writes texture coordinates and colour on every rebuild, before the positions |
| `Anm_DrawRingStrip` 0x464C60 | draws the strip (see below) |
| `Anm_RingSetUV_Alt` 0x4649A0 | only called once by `EffInit_Ring`, with 6 vertices; overwritten at the first draw, so it is irrelevant |

`Anm_DrawRingStrip` details:
- `DrawPrimitiveUP(TRIANGLESTRIP, 2n primitives, 2n+2 vertices, stride 0x1C)`;
- FVF 0x144 = XYZRHW | DIFFUSE | TEX1;
- vertex layout: x, y, z, rhw = 1, colour, u, v.
- The VM's blend mode and texture are applied. Texture address mode is WRAP (05 §12.4, set once at device
  init and never changed).

### Vertex layout

Let n = segments (+0x324). Pairs k = 0 … n (n + 1 pairs; the last pair closes the ring).

| | even vertex 2k (outer edge) | odd vertex 2k+1 (inner edge) |
|---|---|---|
| position | centre + polar(θ_k, r + w) | centre + polar(θ_k, r − w) |
| u | sprite.u0 + uv_off_u | sprite.u1 + uv_off_u |
| v | sprite.v1 + uv_off_v − k·(v1 − v0)/n | same as the even vertex |
| colour | VM colour (+0x1F0, includes alpha) | same |

- sprite.u0, v0, u1, v1 are the **normalised** sprite rectangle (sprite record +0x20…+0x2C).
- uv_off is VM +0x30 / +0x34; it scrolls when the script uses ops 26/27 or uv scroll.
- θ_k = start angle (+0x318) + k·2π/n. Any θ ≥ π is reduced by 2π before use.
- Positions are pre-transformed screen coordinates: centre +0x2E0 + playfield origin (32, 16), z = 0.
- The ellipse and wavy variants (13 §2.5) only change the positions. The UVs are the same.

**So the sprite's v axis (the 768-px axis of sprite 221) runs along the ring.** It goes from v1 down to v0
over the full circle. The u axis (16 px) runs across the band: outer edge = u0, inner edge = u1.

For sprite 221 in the 128×128 etama3.png (rectangle 0, 0, 16, 768):
- u0 = 0, u1 = 0.125;
- v0 = 0, v1 = 6.0.

With WRAP addressing the 128-px texture column repeats **6 times around the ring**. There is one set of
v coordinates for the whole ring, not per segment.

Other sprites:

| sprite | v range | repeats |
|---|---|---|
| 223 | 6.0 | 6× |
| 226 (death ring, 16×128) | 1.0 | 1× |
| 227 (16×256) | 2.0 | 2× |

### Width

`w = width / sin((π − 2π/n)/2)` = width / cos(π/n).
- width = +0x320, copied by the update from VM scale.x (+0x18).
- radius = +0x314, copied from VM pos.x (+0x208).

Order within a frame: the update runs before `AnmVm_Run` in `EffectMgr_Calc`, so the ring uses the VM
values from the previous frame's script step (a 1-frame lag).

### Spell-card start ring (type 39, or type 52 in special mode, fixed slot 1) — `Spell_Start` 0x4152A0

- Type = 39, or 52 when game flags bits 7–8 ≠ 0. Colour 0xFFFFFFFF, position = boss position.
- Update `EffUpd_RingSimple` 0x427990 copies only width (scale.x) and radius (pos.x). Amplitude
  (+0x32C) stays 0, so this is a **plain ring**. The "waves 6" (+0x334 = 6.0) and VM pos.y written by
  `Spell_Start` have no effect.
- n = 64, start angle 0.
- Radius: VM pos.x interpolates **8 → 256 over 100 frames**, mode 6 = 1 − (1 − t)⁴. Pos timer set to 0,
  duration 100. Start (8, 64), end (256, 0); y is unused.
- Script 76 (type 39) / 77 (type 52):
  - sprite 221 / 223, additive;
  - alpha 0, then fades to 255 over 70 frames;
  - scale.x 192 → 15 (script 76) or → 20 (script 77) over 120 frames, linear (mode 0);
  - then loops (`nop` / jump keeps the VM alive);
  - scale.y = 1 is unused.
- **Yes:** 192 is the initial width input. The ring starts as a band about 192 px half-wide; with
  r = 8 the inner edge is at negative radius (crossing the centre).
- Scroll: scripts 76/77 contain no uv op, so uv_off = 0 and there is no scroll.
- `Spell_End` changes the alpha (13 §2.7).

### Death ring (type 59, fixed slot 11) — `Player_Hit` path (decompile around player +0xE2B28)

- Spawned with colour 0xFFF0404F and +0x357 = 1 (it keeps updating while bullets are frozen).
- n = 64, angle 0, width parameter 15, +0x334 = 6. Width and waves are overwritten or ignored as above.
- Radius: VM pos.x **128 → 8** (shrinking) over D frames, mode 4 = 1 − (1 − t)². VM pos starts at (128, 32).
- D = the deathbomb window length (player +0xE2A68):
  - 2 frames when bombs < 1;
  - else bombs·6, +7 if stats.time ≥ stats.threshold, capped at 15;
  - doubled during a spell (capped at 30);
  - × 9/5 for shot types 0, 4, 5.
- Game flag 0x400 (bullets frozen) is set during a spell.
- Script 78 (sprite 226):
  - additive;
  - alpha 0 → 255 over 4 frames;
  - scale.x 20 → 8 over 120 frames;
  - loops.
- Mapping: sprite 226 has v range 1.0, so **one** copy of the 128-px texture goes around the ring, and u
  spans 16 px across the band.
- This answers question 3: the "partial arc" in the port is the same axis bug as the spell ring.

### Port fix

- Put v along the ring: v_k = v1 + uv_off_v − k·(v1 − v0)/n, the same for both vertices of a pair.
- Put u across the band: outer u0, inner u1 (+ uv_off_u).
- Use WRAP sampling.
- Ring 39/52 radius uses ease mode 6; ring 59 shrinks 128 → 8 with ease mode 4.

---

## 3. Death ring

See priority 2. VERIFIED.

## 4. Particle placement, types 19 / 51 / 63 — VERIFIED

### Notation

- C = stage camera position (`g_Stage`+0x6394 = 0x4EA3C4).
- F = camera facing vector (+0x63A0 = 0x4EA3D0, not normalised).
- R = Rng_FloatRange(x) = R·x.
- S(x) = Rng_SignedRange(x) = (2R − 1)·x.
- param = the spawn parameter (+0x2B0; zero for plain `Effect_Spawn`).

The base point is C + F + (−F)/2 = **C + F/2**. The world position (+0x2D4) = base + the offsets below.
After the draws, v and a are multiplied by game speed. Layer = 1 (billboard in list B).

| draw | type 19 `EffInit_Petal3D` 0x426E70 | type 51 `EffInit_Particle51` 0x426280 | type 63 `EffInit_Particle63` 0x426720 |
|---|---|---|---|
| 1 x | R(120) − 60 | S(60) | S(60) |
| 2 y | R(200) − 100 | S(100) − 50 | S(200) − 200 |
| 3 z | R(100) − 100 | R(100) − 100 | R(100) − 100 |
| 4 vx | R(0.06) − 0.03 + param.x | S(0.001) + param.x | S(0.001) + param.x |
| 5 vy | R(0.06) − 0.03 + param.y | S(0.03) + param.y | S(0.03) + 0.4 (param.y ignored) |
| 6 vz | R(0.1) + 0.03 + param.z | −R(0.1) − 0.3 + param.z | −R(0.1) − 0.3 + param.z |
| 7 ax | R(0.0002) − 0.0001 | S(0.0001) | S(0.0001) |
| 8 ay | R(0.0002) − 0.0001 | S(0.0001) | S(0.0001) |
| az | 0 | −0.0003 (constant) | −0.0003 |
| 9 | rotation z = R(2π) − π | — | — |
| 10 | spin (stored in VM rotation.x) = R(0.0314159) − 0.015708 | — | — |

So type 19 uses 10 draws, and types 51 and 63 use 8 draws, not 9. Correction: 13 §2.6 said 9.

Types 51 and 63 also set:
- pos offset (+0x288) = (−9999, …) as an "unset" marker;
- VM pos_start / pos_end (+0x238…+0x258) cleared (they are reused as accumulators, see 8).

### Per frame (all three)

```
v += a
world += v
pos = world
d = normalise(pos − C)
if dot(d, normalised facing (+0x63B8 = 0x4EA3E8)) < 0.94: free
```

Then, per type:
- **Type 19** (`EffUpd_Petal3D` 0x426D70): rotation z = normalise(rotation z + spin). It is kept only
  while pos.z < 0; it is freed once z ≥ 0.
- **Types 51/63** (`EffUpd_Particle51` 0x4264F0 / `EffUpd_Particle63` 0x426990): if a boss exists
  (`0xF54CC0[0]` ≠ 0) and boss flag A bit 3 is set:
  - pos offset := boss position on the first frame (offset.x == −9999);
  - after that, offset += (boss pos − offset)·0.1.
  The offset is only consumed by the screen-space helper (8).
- **Type 51 only:** VM flag 0x20000 is set, and colour2 = VM colour × stage overlay colour
  (`g_Stage`+0xA34 = 0x4E4A64, per byte, (a·b) >> 8).

---

## 5. ECL op 128 / effect 13 — VERIFIED

### Spawn (op 128, in `Ecl_Run`)

`Effect_Spawn(13, enemy pos, 1, 0xFF6060D0)`, then:
- axis (+0x2EC) = (a, b, c);
- enemy +0x53C4 = d (target radius);
- the effect pointer is appended to enemy +0x5360[], count +0x53C0.

### Init `EffInit_Orbit13` 0x425FE0

- layer 2;
- axis := 0;
- radius +0x314 := 0.

The spawn then overwrites the axis.

### Enemy side, every frame (`Enemy_UpdateOrbitEffects` 0x42E010)

For each handle:
- visible bit = !(enemy flag A bit 4);
- centre (+0x2E0) = enemy pos;
- radius: if d ≤ radius then radius = d, else radius += 0.3;
- angle (+0x318) = normalise(angle + 0.0314159).

When the enemy dies, `Enemy_ReleaseOrbitEffects` 0x42A820 sets each effect's fade request (+0x352 = 1) and
clears the list.

### Effect update `EffUpd_Orbit13` 0x426030

```
n = normalise(axis)
q = (n·sin(angle), cos(angle))          # full angle, not half: D3DX rotation-from-quaternion of this q
                                        # rotates by 2·angle about n
p = n × (0,0,1) = (n.y, −n.x, 0)
if |p|² ≥ 1e−5: p = normalise(p)
else: p stays as is (≈ 0)               # quirk: the code writes (1,0,0) into the wrong variable
p = p · radius
p = transform(p, rotation matrix of q)
p.z ·= 6
pos = centre + p
pos.z = 0
```

Fade, when +0x352 is set:
- +0x353 += 1;
- if it is > 15: free;
- f = 1 − n/16;
- alpha = trunc(f·255);
- scale.x = scale.y = 2 − f.

---

## 6. Natives 3 / 6 / 20 — VERIFIED

- **Effects:** both effects are the **same type**:
  - native 3: type 0x38 (56) in fixed slots 9 and 10;
  - native 6: type 0x3A (58);
  - native 20: type 0x41 (65).

  All use colour 0xFFFFFFFF at the enemy position. The slot-10 effect then gets etama script 0x61 / 0x65 /
  99 (`AnmFile_SetScript` on the etama file). Native 5 ends both. The port is correct.
- **Stage callback** `Stage_SpellMaskCallback` 0x4235A0 is stored in `g_Stage`+0x625C (0x4EA28C).
  `Stage_DrawHigh` calls it after drawing the spell-background VMs, only while `g_Stage`+0xB24 > 0.
  `Spell_Start` clears it. Steps:
  1. a = effect 9's VM pos.x · √½ and b = effect 10's VM pos.x · √½. These are the ring radii (update
     0x4279D0: radius := pos.x), so the squares are inscribed in the rings. c = effect 9 pos + (32, 16).
  2. Draw a 10-vertex triangle strip, opaque **black** (0xFF000000), z 0.8, rhw 1, with FVF XYZRHW|DIFFUSE.
     It is a square frame between the square of half-size a and the one of half-size b around c; vertex
     order (c−a), (c−b), (+a,−a), (+b,−b), (+a,+a), (+b,+b), (−a,+a), (−b,+b), back to the start.
     Render state:
     - colour/alpha op SELECTARG1 with arg DIFFUSE (no texture);
     - z-write on;
     - ZFUNC unchanged (ALWAYS).
  3. Restore texture stage 0 to MODULATE / TEXTURE, invalidate the ANM state caches, and set ZFUNC
     LESSEQUAL.
  4. Redraw spell-background VM 0 (`g_Stage`+0xB38) mirrored:
     - scale (−1.5, −1.75), pos (416, 464, 0.7), colour 0xFFE0C0C0, `Draw2DRot`;
     - then restore scale (1.5, 1.75), pos (32, 16, 0.5) and the colour.
  5. Redraw VM 1 (+0xDDC):
     - rotation z negated, pos.z 0.6, colour 0xFFE0C0C0, `DrawRotFlush`;
     - then rotation negated back, pos.z 0.5, colour restored.
  6. ZFUNC ALWAYS.

  The visual intent (an inner square showing a mirrored background) is INFERRED. The steps above are
  VERIFIED.

---

## 7. Table 0x4C7480 — VERIFIED

Indexed by `g_StageIdx` (0x164D2CC, 0–8):

| index | file |
|---|---|
| 0 | eff01 |
| 1 | eff02 |
| 2 | eff03 |
| 3 | eff04a |
| 4 | eff04b |
| 5 | eff05 |
| 6 | eff06 |
| 7 | eff07 |
| 8 | eff08 |
| 9–11 | eff09sk / ym / al (not reachable through this index) |

The port is correct.

Spell practice with card id ≥ 0xD8 (216) uses table 0x4C7144[id]:

| card | file |
|---|---|
| 216 | eff09sk |
| 217 | eff09ym |
| 218 | eff09al |
| 219 | eff09rm |
| 220 | eff09yy |
| 221 | eff09yk |

The load is skipped while the reload-same-stage flag 0x17CE8C4 is set (`EffectMgr_OnRegister` 0x4284B0).

---

## 8. Draw list B (background billboards) — VERIFIED

### Where it is drawn

`EffectMgr_DrawBackground` 0x4281E0 is called from `Stage_DrawHigh` after layers 2 and 3. The stage camera
is still set (view 0x17CE7A0, projection 0x17CE7E0, viewport 0x17CE820), and **fog has just been turned
off**. There is no fog tint on any list-B effect.

It only runs inside the `+0xB24 < 2` and HUD-condition block that also gates layers 2/3.

### Effect level (cfg +0x24 / byte 0x17CE894)

| level | result |
|---|---|
| 0 | nothing |
| **1** | **nothing either.** The loop toggles a parity flag and *returns* on the first element (asm 0x428224–0x42823F), so it does not draw every other element. Correction to 13 §2.4. |
| 2 | every element |

### Per element

VM pos := effect pos, then:

| layer | draw |
|---|---|
| 4 | `Draw2DRot` (screen coordinates, no projection) |
| 1, types 0x33 / 0x3F (51 / 63) | 0x464400 → 0x4640E0 with callback 0x428310 |
| 1, other types | `AnmManager_DrawBillboard` 0x463CF0 |
| else | `AnmManager_Draw3DQuad` |

**Billboard projection (0x463CF0 / 0x4640E0):**
- s = D3DXVec3Project(pos) with world = translation(pos).
- If s.z ∉ [0, 1] the element is culled.
- Half-size = 0.5·|project(pos + right) − s|·sprite_w·scale.x, and the same with sprite_h·scale.y.
- right = `g_Stage`+0x63C4 (0x4EA3F4) = normalise(cross(facing, up)). **`Stage_SetCamera` 0x40B6D0
  writes it every frame.** Correction to 05 §9 / NOTES: the vector is not zero and the billboard is not
  degenerate.
- The rotation is VM rotation.z; the vertex z is s.z.

**Callback 0x428310 for types 51 / 63.** It moves the projected point s in screen space; skipped while
paused or in the continue menu (0x164D0BA / 0x164D0BB):

```
p = s + D            # D = VM pos_end (+0x244), accumulated displacement
if pos_offset.x > −9999 (boss pull active):
    d = pos_offset − p + (32, 16)
    if |d|² < 160²:
        speed (+0x238) += 0.0005
        D += d·speed
d2 = p − player pos (0x17D61AC) − (32, 16)
if |d2|² < 88²:
    D += d2·0.02     # pushed away from the player
s += D
```

---

## 9. Auras and invulnerability ring — VERIFIED

All three follow the player every frame:

| effect | how it follows | when it ends |
|---|---|---|
| **Youkai aura** (type 22, slot 2; spawned in `Player_Move` 0x44AEC0) | its update `EffUpd_FollowPlayer` 0x426C40 sets pos = player pos (0x17D61AC) each frame while its script runs (VM +0x220 ≠ 0) | freed when the script ends |
| **Gauge-max aura** (type 25, slot 8) | `Player_Move` sets the effect pos = player pos (+0x2B4) every frame after movement, through the handle at player +0xE2B24 | when the gauge is neither human-max nor youkai-max, the effect is deactivated (+0x350 = 0) and the handle cleared |
| **Invulnerability ring** (type 23, slot 0; `Player_SpawnInvulnRing` 0x40BF00, handle player +0xE2B1C) | its update is a no-op; `Player_UpdateInvuln` 0x44D2C0 copies the player pos into it every frame while player state = 3 | when the invulnerability timer (+0xE2AF4) reaches < 1: deactivated, handle cleared, state 0 |

Order: player calc runs before `EffectMgr_Calc` (prio 13), so the effects use the current frame's player
position.

---

## 10. Stage sparkles — VERIFIED

### Collecting points

In `Stage_DrawLayer` 0x40A1B0 (billboard path, 06 §11.1), every drawn quad records its projected screen
position (s.x, s.y, 0) when all of these hold:
- VM tag (+0x1FC) & 0xF0 == 0x10;
- a value < fog near (+0xAEC): the value is the eye distance when that distance ≤ fog near, otherwise
  the fog fraction (< 1), so practically every visible tagged quad passes;
- +0x647C ≠ 0.

The point is written to `g_Stage`+0x6480 + count·0xC, and count (+0x6478) += 1. There is no bounds check;
the array holds 32.

Resets:
- `Stage_DrawLow` resets count = 0 and zeroes the first 16 points every frame.
- `Stage_Calc` sets +0x647C = 1.
- `Stage_DrawHigh` clears it at the end.
- count = 0 when +0xB24 > 1.

### Spawning (`Stage_Calc` 0x407400)

Every frame where:
- frame counter (+0x81C) % 3 == 0;
- and (counter > 699 or spell practice);
- and +0xB24 < 2,

spawn type 0x3E at **all 12 points 0…11** (fixed 12, regardless of count), colour 0x20FFFFFF, layer 4.
Unused points are (0, 0, 0) from the per-frame clear, so those sparkles appear at screen (0, 0) under the
HUD frame. This is a faithful quirk; each spawn costs the RNG draws of type 0x3E's init, if any.

---

## 11. Screen-shake consumers — VERIFIED

The offset (AnmManager +0x1C / +0x20) is read in exactly three places:

1. `AnmManager_EmitQuad` 0x4628B0, which every 2D quad goes through. That covers:
   - all 2D modes;
   - projected billboards (stage objects and list B);
   - mode 5 / `Draw3DQuad` (projected first, then emitted as 2D);
   - ASCII popups (ASCII strings are drawn at prio 20, after the reset, so they do not shake).
   The offset is added in **screen space**.
2. `AnmManager_Draw3DWorld` 0x464470 adds it to the world translation **before** projection (06 §11.2).
   On distant stage geometry the screen motion is therefore much smaller than the offset. The original
   behaves the same way, so the port's "no visible background shake" is faithful.
3. The Ascii player-spotlight rectangles (0x405420, `Screen_FillRect` around the player).

**The offset is zeroed every frame by the popup draw.** 0x405420 runs at draw prio 14 and sets
AnmManager +0x1C / +0x20 = 0 when it finishes. So:
- only draws with prio ≤ 14 in a frame are shaken: stage, effects, enemies, bullets, player, popups;
- the HUD (prio 17) and ASCII strings (prio 20) are never shaken;
- the shake effect (calc prio 3) writes a new offset every frame while it is active;
- when it ends, the offset is 0 from the next frame on. The 13 §3 note "offset left as is" therefore
  lasts at most one frame's prio ≤ 14 draws.

Not shaken:
- the ring strip (`Anm_DrawRingStrip`);
- `Stage_SpellMaskCallback`;
- `Screen_FillRect` fades and the screen effects.

---

## 12. Popup colours — VERIFIED

| caller | value | colour |
|---|---|---|
| small power (`0x440CF0`, type 0) | 10, or −1 (PowerUp) when power crosses a level of table 0x4C7A6C | 10: 0xFFFFFFFF; PowerUp: **0xFFFFC0A0** (+ sfx 0x1F) |
| big power (`0x441170`, type 2) | same rule | same |
| full power (type 4) | −1 if power was < 128 (also cancels bullets), then 1000 | 0xFFFFC0A0, then 0xFFFFFFFF |
| point (`Item_CollectPoint`, type 1) | value (÷10 rules 08 §8.5; ×2 at human max) | 0xFFFFFFFF; **0xFFFFFF00** (yellow) when value ≥ max |
| converted point (0x441020, type 8) | same rule | same |
| cancel star (type 6, `Popup_NumberBig`) | (stats+0xC / 40)·10 + 300 (min 10), or 100 when 0x18B8974 ≠ 0 | 0xFFFFFFFF |
| time orb (`0x4412B0`, type 7, `Popup_NumberBig`) | see 08 §8.5 | **0xDFFFEF80** when stats.time ≥ stats.threshold (stats +0x3C ≥ +0x40), else **0xDFFFFFFF** |
| cancel bonus (`BulletMgr_CancelBonus`) | step value | white; 0xFFFFFF00 when value ≥ cap |
| kill_all (`EnemyMgr_KillAll`) | step value | white; 0xFFFFFF00 when value ≥ cap |

The colour expression `((v < max) − 1 & 0xFFFFFF01) − 1` evaluates to:
- 0xFFFFFFFF when v < max;
- 0xFFFFFF00 when v ≥ max.

So the port's guesses need these corrections:
- max = 0xFFFFFF00 (not 0xFFFFFF40);
- PowerUp = 0xFFFFC0A0;
- time orb = 0xDFFFEF80 / 0xDFFFFFFF.

Large-popup colours are in 14.

---

## 13. Glyph 10 "PowerUp" — VERIFIED

`Popup_Number` with a negative value stores the single glyph 10.

Draw (0x405420):
- x starts at pos.x − 4·count and advances **8 px per glyph regardless of the sprite width**.
- The Ascii VM is anchored top-left (flags |= 0x1800 in 0x402450), so each glyph covers
  [x, x + sprite_w] × [y, y + h].
- Glyph 10 (sprite 10, 48×8) is therefore drawn at **[pos.x − 4, pos.x + 44]**: left-aligned, not centred.

Fade sprites are glyph + 11 (age 52–55) and glyph + 21 (age ≥ 56). For glyph 10 those are:
- **sprite 21** (8×8 at (128, 8));
- **sprite 31** (16×16 at (0, 32)).

These are not PowerUp variants. The original really shows them for the last 8 frames. Port faithfully: use
sprite 21 at ages 52–55 and sprite 31 from 56, same anchor.

Alpha: by squared distance to the player (13 §4).

---

## 14. Callers of 0x403330 and the large popups — VERIFIED

### `Popup_NumberBig` 0x403330 (3 slots 720–722, round robin)

- cancel star item (type 6) in `ItemMgr_Calc`;
- time orb `0x4412B0` (type 7).

Both are listed in 12.

### Large popups

Format: glyphs stored least significant first:
- if p4 > 0: 0xF, then the digits of p4, then 0xE;
- then the digits of p3 (or 0);
- then 0xD.

ascii.png strip y = 208 (sprites 0x88 + g): 0xD = '+', 0xE = '(', 0xF = ')'. The text reads **`+p3`** or
**`+p3(p4)`**.

`Popup_Large` 0x403460 (life 90), callers in `Enemy_KillChildren` 0x42ADB0:

| case | p3 | p4 | colour | scale |
|---|---|---|---|---|
| each killed chain child | chain count n (÷3 when 0x17D6ED4 ≠ 0) | 0 | see below | manager scale |
| boss chain kill | boss +0x3380 | 0 | 0xFFF0F00F | 2.0, then 1.0 restored |
| single kill | 1 | 0 | 0xFFFFFFFF | — |

Chain child colour (n = `FUN_0041FD40` = number of chained children):

| n | colour |
|---|---|
| < 2 | 0xFFFFFFFF |
| 2–5 | 0xFFFFFFD0 |
| 6–9 | 0xFFFFFFB0 |
| ≥ 10 | 0xFFFFFF80 |

- The boss chain kill also spawns 2·(+0x3380) time orbs within 128 px.
- The single-kill popup comes from a child with flag +0x2DA4 when its parent dies: gauge −1/12, +1 time
  orb.

`Popup_LargeStatic` 0x403600 (starts at age 88, i.e. a 2-frame life re-issued every frame), caller
`Hud_DrawBoss`:
- shown when not paused, not in the continue menu, bullets not frozen, and a boss exists;
- screen position (2, 29), scale 1.0;
- p3 = number of chained children of the boss, p4 = boss +0x3380;
- colour 0xFFF0F00F.

---

## 15. Title-menu description text — VERIFIED

### The VMs

14 VMs at menu +0xC2C0 + i·0x2A4, created once by the menu loader thread (0x470E10, `MainMenu_LoaderThread`)
from **text.anm script 9** (slot 0 file 0x17CE8F4; `AnmFile_StartScript` with scripts[9]). Setup:
- VM +0x21A = 9;
- then sprite := VM +0x218 base + i, i.e. sprite 21 + i (the text.anm render-target sprites).

Script 9:
- sprite 21 at **pos (320, 448)**, centred (no anchor op);
- interrupt 1: alpha 0 → 255 over 20 frames;
- interrupt 2: alpha → 0 over 60 frames, then hide.

### Text

`Text_RenderCentered` 0x466650 renders into the VM's sprite:
- colour 0xFFF0E0;
- shadow/outline flag 0x300000;
- strings from table 0x4C8640 (top menu, 9 entries), 0x4C8664 (options, 10).

### Behaviour

- Only the current description VM (+0xC2BC) is run and drawn.
- On a cursor change it becomes the new item's VM and gets interrupt 1 (+0x1FE = 1).
- The old one is simply no longer run.

**Port:** draw at (320, 448) centred, not a raster at (128, 440).

---

## 16. Option screen value VMs — VERIFIED (`MainMenu_Option` 0x46837B)

title01 VM indices (+0x218 = the VM's base sprite; "lit" = base, "dim" = base + 1):

| VMs | value | rule |
|---|---|---|
| 20–26 | lives 0…6 (cfg +0x1C, byte 0x17CE88C) | all dim, VM 20 + lives lit. VM 25 is hidden (visible bit cleared) while play count 0x164CED0 < 30, VM 26 while < 60 |
| 27–28 | colour cfg +0x1E: 0 = 32-bit (normal), 1 = force 16-bit | |
| 29–31 | music cfg +0x1F: 0 off, 1 WAV, 2 MIDI | |
| 32–35 | BGM volume (byte 0x17CE897) | VM 32 hundreds (shown only if ≥ 100, base sprite), VM 33 tens (base + tens·2; hidden if < 10), VM 34 ones (base + ones·2), VM 35 '%' |
| 36–39 | SE volume (0x17CE898) | same as BGM |
| 40–41 | window cfg +0x22 | |
| 42–43 | slow cfg +0x25 | |
| 10–19 | menu item labels | dim except the cursor |

- **The value VM is VM (first + value)**, not "value → sprite v + 1".
- **Lives max:** 4, 5 or 6 by play count (< 30, < 60, else); see 15 §1.3. Down-wrap from 0 goes to the
  max.
- **Colour mode:** 0 = 32-bit.

---

## 17. Unlock rules — VERIFIED (15 §2.4, 16 §4)

**Top menu:**
- Extra (item 1, VM 2) is greyed 0xFF404040 and skipped unless `Clear_ExtraUnlocked` 0x43C1E9: some team
  0–3 has a no-continue clear with the 0x4000 (Final B ending) bit in any difficulty E–L.
- Spell Practice (item 2, VM 3) is greyed unless `Clear_AnyFinalA` 0x43C2C5: some team has the 0x8000
  (Final A) bit in the any-clear mask.

**Characters:**
- Solo characters (team indices 4–11) are available when `Clear_AllTeamsFinalB` 0x447D04: all four teams
  pass Final B.
- In spell practice, the character cursor wraps over 4 teams (12 with solos) and skips any team t < 4
  without `Clear_TeamFinalA(t)`.

**Spell practice cards** (`Card_IsSelectable` 0x46FD5F) are selectable if:
- id < 205 and the card was attempted in story or practice (CATK attempt counters ≠ 0); or
- it is a Last Word whose FLSP slot holds its own id (`LastWord_ComputeUnlocks`, 15 §2.8).

**Practice stages:** the any-clear mask for (shot, difficulty) (12 §2.5).

---

## 18. Boot logo time and missing config — VERIFIED

- **Logo:** there is no fixed hold time. The logo (surface 8) stays up while the loading state is 1/2.
  `MainMenu_LoaderThread`:
  1. sleeps until the ANM manager has no pending load (AnmManager +8 < 0);
  2. loads title01.anm, resulttext.anm and title00.png;
  3. starts the title BGM (slot 8, `bgm/th08_01.mid`, unless already playing) and a fade-from-white
     screen effect (type 0, prio 0x15);
  4. calls `Loading_End`.

  The duration is the load time. Any short hold (e.g. 1 frame after loading) is faithful.
- **Missing or invalid th08.cfg:** every field takes the defaults in 15 §1.1. Lives 2, music 1 (WAV when
  thbgm.dat is valid, else 2), SE on, difficulty 1, effect level 2, BGM 100, SE 80, debug flags bit 0.
  The file is written back immediately.

---

## Corrections applied to earlier specs (this pass)

| spec | change |
|---|---|
| 13 §2.4 | effect level 1 draws no list-B effects, not every other one |
| 13 §2.4 | no fog tint; types 51/63 use the screen-space helper (8) |
| 13 §2.5 | v runs along the ring and u across; WRAP |
| 13 §2.6 | type 3 is a bomb-area spark only; 51/63 use 8 RNG draws; type 19 is freed when z ≥ 0 |
| 13 §2.7 | spell ring radius ease mode 6 and plain ring (no amplitude); death ring 128 → 8 |
| 13 §2.7 | hit spark row corrected |
| 13 §4 | large-popup format; colours |
| 13 §6 | open point closed |
| 05 §9 (mode 4) and NOTES | billboard reference vector = camera right, written by `Stage_SetCamera` |
| 12 §2.2 | description VM position |
| 12 §2.7 | value VM mapping |
