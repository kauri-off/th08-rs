# Draw log format (autocmp)

A draw log lists, for every presented frame, everything the game put on the 640×480 screen, in draw order.
Reference logs in this directory come from the original th08.exe (wine + probe, `re/TOOLING_REPORT.md` §6).
The port writes the same format, and `re/tools/drawlog.py diff` compares the two logs.
The format describes **what the game drew**: textures by file name, screen-space vertices, effective colour,
and blend/test state. You do not need to know D3D to read or write it.

Files are plain text, UTF-8/ASCII, one record per line, fields separated by single spaces. They are stored
`xz`-compressed (`*.drawlog.xz`). The tools read `.xz` and plain text.
Numbers are decimal with trailing zeros removed (`436.5`, `0`, `0.00781`). Compare them with a tolerance, not as
text. Colours are 8 hex digits **AARRGGBB**.

## 1. Frames

```
F <present> <scene> <scene_frame> <game_frame> <demo> <demo_frame> <input> <nquads>
```

One `F` line starts each presented frame. All the following records up to the next `F` belong to that frame.

| field | meaning |
|---|---|
| present | presented-frame counter. 1 = first Present after process start |
| scene | Supervisor scene id (0x17CE8B0): 0 boot/logo, 1 main menu, 2 game, 5 result viewer, 8 music room, … (00_overview) |
| scene_frame | frames presented since the scene id last changed (1 = first frame of the scene) |
| game_frame | GameManager frame counter 0x164D2C8 (0 outside gameplay) |
| demo | attract-demo index 0–3 while a demo plays (game flags bit 1), else −1 |
| demo_frame | replay frame 0x164D0C0 while a demo plays (same numbering as `specs/data/demo*_trace.txt` column 2), else 0 |
| input | the 16-bit input word the game used for this frame (0x164D528, 12 §2 bits) |
| nquads | number of `R`/`Q` records in the frame |

All values are sampled when the frame is presented. That is after the frame's calc pass(es) and draw. During
replay fast-forward several calc passes run per presented frame, so `demo_frame` can skip.

## 2. State records (effective state; emitted only when it changes)

The state lines say what applies to the primitives that follow. A writer emits `T`, `S`, `V` whenever the
effective value differs from the last one written **in this frame**. At each `F` all three are reset, so the first
primitive of a frame is always preceded by `T`, `S`, `V`. The order of the three is not significant.

```
T <label> <w> <h>          texture; "-" = none
S <key=value> ...          blend / tests / colour combine
V <x> <y> <w> <h> [<minz> <maxz>]   viewport in screen pixels (minz/maxz only if not 0/1)
```

**Texture label**: `<anm file name>#<entry index>`, e.g. `front.anm#0`, `etama.anm#1`, `text.anm#0`
(the `@` render-target entries keep their anm label). `w h` = the entry's **logical** size from the ANM header
(03 §2.1 +0x0C/+0x10), so `u·w`, `v·h` are texel coordinates of the sprite rect. A texture that cannot be
attributed is written `?<hex> 0 0`. No such label appears in the reference logs.

**S keys** (always written in this order):

| key | values |
|---|---|
| `blend` | `alpha` = src·a + dst·(1−a); `add` = dst + src·a; `off` = no blending; else `<src>,<dst>` factor names (`one`, `zero`, `srcalpha`, `invsrcalpha`, `srccolor`, `invsrccolor`, `destcolor`, …) |
| `atest` | `off`, or `<cmp><ref>`: the pixel is discarded unless alpha·255 `<cmp>` ref (`ge4` = keep alpha ≥ 4) |
| `ztest` | `off` (depth test disabled), `always`, or `le`/`lt`/… against the depth buffer |
| `zwrite` | 1/0: depth is written when depth testing is enabled (ztest ≠ off) |
| `c` | colour combine: `tex*col` (texture × vertex colour), `tex`, `col`, `tex+col`, `off` |
| `a` | alpha combine, same names |
| `fog` | present only when vertex fog is on: `<AARRGGBB colour>,<start>,<end>`. Linear stage fog (06 §8, VERIFIED): per **pixel** f = clamp((end − d)/(end − start), 0, 1), rgb = f·rgb + (1−f)·fog rgb, alpha unchanged. d = the pixel's eye-space depth: interpolate d perspective-correctly after clipping to 0 ≤ z ≤ w, and clamp after interpolating, not per vertex. Only stage quads carry fog. For them d = 1/rhw = 30·1800/(1800 − 1770·z). Use the z form when reading a log: rhw is rounded to 4 decimals, which is too coarse (±4 % of d) |

`col` is the effective vertex colour after the game's colour path. The original has two paths (05 §12.4).
One puts the colour in each vertex. The other uses one constant "texture factor" colour for the whole draw.
The log always folds the constant into the per-vertex colour, so a writer does not need to know which path was
used.

## 3. Primitive records

```
R <x0> <y0> <x1> <y1> <u0> <v0> <u1> <v1> <colour> [<z>]
Q <v0> <v1> <v2> <v3>                       each v = <x> <y> <z> <rhw> <u> <v> <colour>
P <kind> <n> <v1> ... <vn>                  kind = strip | fan | list | points | lines
K <src> <dst> <sx> <sy> <w> <h> <dx> <dy>
C <t|z|tz> <colour> [<x>,<y>,<w>,<h> ...]
```

- **Quads** are the game's sprite quads. The vertex order is top-left, top-right, bottom-left, bottom-right
  of the *sprite* (05 §9), drawn as triangles (v0 v1 v2) and (v1 v2 v3). Coordinates are screen pixels in
  the D3D8 convention: a pixel's centre is at integer coordinates. That is why snapped sprites sit at `n.5`
  edges (05 §9 mode 0).
  - `R` is the compact form. Use it when the quad is an axis-aligned rectangle (v0/v1 share y, v0/v2 share x,
    and so on), rhw = 1 at all four corners, the four corners share one z, the UVs form the matching rect, and
    all four colours are equal. `x0 y0` belong to v0, `x1 y1` to v3, and z is omitted when 0.
    A mirrored sprite (negative scale) still fits `R`, with x1 < x0.
  - Otherwise use `Q` with all 4 vertices.
  - A writer must use `R` exactly when these conditions hold. The diff tool expands both forms to vertices, so a
    mismatch in form alone does not count as a difference.
- **3D quads** (stage geometry `Draw3DWorld`, draw modes 4/5, rings) are logged **after projection to the
  screen**: x, y = viewport pixel position, z = depth in [0, 1], rhw = 1/w_clip. Geometry behind the camera can
  give large or negative values. These are clipped later and are logged unclipped. For 3D draws, u v are the final
  texture coordinates, after the texture matrix that `Draw3DWorld` uses (06 §11.2).
- Untextured quads (e.g. `Screen_FillRect`, T `-`) are written as `Q` with `-` for u and v.
- **P**: any other primitive, with all its vertices. These are the ring/strip effects (`Anm_DrawRingStrip`,
  triangle strips with more than 4 vertices) and fans.
  A triangle list is split into quads whenever two consecutive triangles have the pattern (a b c)(b c d).
  The game's batched sprites always do, so they appear as `R`/`Q`.
- **K**: a pixel copy to or from the screen, with no filtering or blending (`Anm_DrawSurface`, the main-menu
  background, 12 §2.2; screen captures). `src`/`dst` = image file name of the surface slot (`title00.png`,
  `th08logo.jpg`, `select00.png`, …), `screen`, or a texture label. `K <src> <dst> full` = whole surface.
- **C**: clear of the current viewport (a `V` line is written before it if the viewport changed). `t` = colour, `z` = depth (depth clears to 1.0), with an optional
  rectangle list.

Records are written in submission order. The original batches sprites with equal state. The log still lists the
quads one by one in draw order, so batching is invisible.

## 4. Persistence (important when rendering or comparing)

The original presents with a *copy* swap (the back buffer keeps its pixels). Every gameplay frame clears depth on the whole screen
but colour only in the playfield viewport (32, 16, 384, 448). The side panel and frame (outside the playfield) are repainted only when
the HUD marks rows dirty (12 §5.3). So most gameplay frames contain no panel records at all. To reproduce a
screen, render all frames from the start of the scene on into the same buffer
(`drawlog.py render --from`). When comparing with a port that redraws the panel every frame, use
`drawlog.py diff --playfield`, which compares only primitives that touch the playfield rectangle.

## 5. Summary

`drawlog.py summary <log>` prints the set of `(texture label, texel rect)` that was ever drawn, with the first
`present` and a count. The texel rect is the bounding box of the UVs × the logical size. `*.summary.txt` next to
each reference log is that output. Comparing two summaries quickly shows sprites the port never draws, or draws
from the wrong rect.

## 6. Writer checklist for the port

1. One `F` per presented frame. Use the same field meanings. Fields the port does not track are written as
   0/−1.
2. Before each primitive, write `T`/`S`/`V` if the effective value changed since the previous primitive of this
   frame.
3. Write quads in the game's submission order. Use screen-space vertices after the global screen-shake offset
   and snapping, i.e. exactly what is rasterised at 640×480.
4. Colour = final per-vertex colour after tint/colour2 selection (05 §12.1–12.2).
5. Stage/3D: project with the game's view/projection/viewport and log the projected vertices (§3).
