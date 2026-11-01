# Step 8 inventory: `Gfx` API and call sites (IMPL161)

Plan: REWRITE_PLAN step 8; design ARCH_A §2.2 / §4 step 8; ARCH_SUMMARY decisions 2, 6, 7; ARCH_D §5, §6.1 item 4.
Core types: `src/drawlist.rs`. Executor: `src/gfx.rs` (`Gfx::sync`, `execute`, `execute_res`).

## 1. `Gfx` pub fns

Class: **D** draw/state command, **S** device-state read, **R** resource create/upload/free (returns an id),
**C** readback/capture, **T** text raster (upload of a CPU raster into an existing texture), **A** app/platform only.

| fn | class | after IMPL161 |
|---|---|---|
| `set_viewport` | D | `DrawCmd::SetViewport`; mirrored in `DeviceState.viewport` |
| `clear_viewport` | D | `DrawCmd::ClearViewport` |
| `sprite` / `quad` / `quad_strip` / `strip` | D | `DrawCmd::Sprite` / `Quad` / `QuadStrip` / `Strip` |
| `set_depth_test` | D | `DrawCmd::SetDepthTest`; mirrored in `DeviceState.depth_test` |
| `clear_depth` / `clear_z` | D | `DrawCmd::ClearDepth` / `ClearZ` |
| `rect` / `solid_quad` | D | `DrawCmd::Rect` / `SolidQuad` |
| `image` / `image_rect` | D | `DrawCmd::Image` / `ImageRect` (K record written by the executor) |
| `fan` | D | `DrawCmd::Fan` |
| `draw_raw` | D | `DrawCmd::Raw` (draw-log renderer only) |
| `set_log_fog` | D | `DrawCmd::SetLogFog`; mirrored in `DeviceState.log_fog` |
| `present` | D | `DrawCmd::Present` + `sync` (the per-frame execution point); mirror reset like the device |
| `viewport` | S | `Recorder::viewport` (mirror) |
| `white` | S | `TexId::WHITE` (slot 0) |
| `texture(id)` | S | `Recorder::texture` → `TexInfo {width, height}` from `ResTable` (was `&Texture`) |
| `create_texture` | R | `ResCmd::Create`; id from `ResTable`; provenance `Bytes` |
| `load_image` | R | decoded in the recorder (size needed by core); `Create` + `SetLabel`; provenance `Image` |
| `free_texture` | R | `ResCmd::Free` (white and double frees filtered by the table, as before) |
| `set_label` | R | `ResCmd::SetLabel`; label also kept in the table |
| `write_texture` | R/T | `ResCmd::Write` (owned pixel copy) |
| `copy_texture` | R | `ResCmd::Copy` |
| `capture_screen` | C | `ResCmd::CaptureScreen` (new id, provenance `Capture`) |
| `capture_rect` | C | `ResCmd::CaptureRect` |
| `flush` / `capture(size)` / `read_texture` | C | tool / screenshot readbacks; `flush` and `capture` `sync` first; stay on `Gfx` |
| `enable_log` / `logging` / `take_log` | C | draw-log tool; `take_log` `sync`s first; stay on `Gfx` |
| `new` / `headless` / `set_vsync` / `attach` / `detach` / `reconfigure` / `surface_format` / `surface_size` / `resize`; fields `surface`, `device`, `queue`, `persistent` | A | stay on `Gfx` (app, launcher, tools) |

Moved to `drawlist.rs` (re-exported from `gfx`): `Vertex`, `Rect`, `Blend`, `LogFog`, `LogTex`, `SCREEN_W/H`,
`FULLSCREEN`, `PLAYFIELD`, `clip_in_viewport`. `gfx::TextureId` is now an alias of `drawlist::TexId(u32)`.
Still in `gfx.rs`: `Mat4`, `Camera` (pure math, but uses std trig → moving it needs fmath; TODO(step 8)).

## 2. Ordering choice

**One list.** `Gfx` owns the `Recorder` for now (`gfx.rec`); every `Gfx` draw/state/resource method is a thin
forward to it, so converted (`&mut Recorder`) and unconverted (`&mut Gfx`) sites append to the same list and the
call order is the recording order. No flush points have to be inserted at call sites.
- `Recorder.draws: Vec<DrawCmd>` and `Recorder.res: Vec<(usize, ResCmd)>`; each `ResCmd` stores the `draws` length
  at record time. `Gfx::sync` replays them merged: every `ResCmd` runs before the draw recorded after it.
- Execution points: `present` (every frame and every in-calc Present), `take_log`, `flush`, `capture`.
  Executing a `DrawCmd` only appends to the CPU batch list (as the direct call did), so deferral changes nothing
  on the GPU; resource commands run before the frame's render submit as before (`write_texture` already applied
  at the next submit, copies/captures read the last presented frame, which is unchanged until the next Present).
- Draw log: produced by the executor from the same code, in call order → byte-identical.
- `TexId`s are allocated by `ResTable` with the device table's old rule (push, or pop the last freed slot; 0 =
  white, never freed), so the `?<id>` names in the draw log are unchanged.
- `anmmgr` fns take `&mut impl AsMut<Recorder>`; `Gfx` and `Recorder` both implement it, so every caller compiles
  unchanged while it is converted file by file.

End state (later runs): when no file takes `&mut Gfx` any more, `rec` moves from `Gfx` to `Ctx` (future World),
the facade block in gfx.rs ("TODO(step 8): facade") is deleted, and the app calls `gfx.execute*` on the drained lists.

## 3. Call-site groups

Counts from `gfx.<fn>(` matches (≈; multi-line calls under-counted). "anm" = calls into the AnmManager draw path
(`draw*`, `load`, `unload`), which already record. Reads back = what the file asks the device/table.

| group | direct `Gfx` calls | anm | reads back | state |
|---|---|---|---|---|
| item | — | `draw_mode_z` | — | **converted** (`&mut Recorder`) |
| anmmgr | — | — | `viewport` (mirror) | **converted** (`impl AsMut<Recorder>`) |
| hud | set_viewport ×5, strip ×3, white ×2, texture ×2, write_texture, copy_texture | ~18 | `texture` size, `white` | T: dialogue/line text upload in calc (decision 7, later run) |
| supervisor | free_texture ×7, set_viewport ×3, load_image ×2, sprite, image, clear_viewport, capture_screen | ~3 | — | R + C |
| stage | set_depth_test ×6, set_viewport ×3, strip ×2, white, viewport, rect, clear_z, clear_viewport, clear_depth | ~8 | `viewport`, `white` | D only |
| menu | free_texture ×3, set_viewport ×2, load_image ×2, image ×2, write_texture, texture, rect | ~6 | `texture` size | R + T |
| ascii | set_viewport ×5, texture, strip, rect, capture_rect | ~13 | `texture` size | D + C (`apply_pending_captures`) |
| result | set_viewport, present, load_image, image, free_texture, capture | ~8 | `capture` (screenshot readback) | R + C |
| musicroom | write_texture, texture, set_viewport, load_image, image, free_texture | ~6 | `texture` size | R + T |
| ending | free_texture ×2, write_texture, texture, set_viewport, rect, load_image, image_rect | ~3 | `texture` size | R + T |
| menulist | write_texture, texture, fan | 1 | `texture` size | T |
| effect | set_viewport ×2, strip, rect | ~6 | — | D only |
| gui | write_texture, texture, set_viewport | ~5 | `texture` size | T |
| enemy | set_viewport | ~7 | — | D only |
| player / pshot / bomb / bullet | set_viewport / — / rect / set_viewport | ~3 / 4 / 3 / ~5 | — | D only |
| gamemgr | set_viewport, clear_z | — | — | D only |
| game (Ctx) | set_viewport ×2, clear_depth ×2, present ×2, enable_log ×2, take_log | unload | `take_log` | D + tool |
| anmdebug (tool) | set_viewport ×2, clear_viewport | 2 | — | D |
| drawreport / drawrun (tool) | draw_raw ×2, texture, set_viewport, load_image, flush, create_texture, clear_z, clear_depth, capture | — | `capture`, `flush` | stays on `Gfx` (renderer tool) |
| app / launcher / launchershot | resize, detach/attach, set_vsync, present, surface_*, reconfigure, read_texture, device/queue | — | surface | A — stays on `Gfx` |
| session / demosync bins | `Gfx::headless`, take | — | — | A |

Suggested split for the next runs (disjoint files): (1) stage + gamemgr + effect; (2) enemy + player + pshot + bomb
+ bullet; (3) ascii + supervisor + result; (4) menu + menulist + musicroom + ending; (5) hud + gui (text raster,
decision 7); (6) game.rs/anmdebug, then move `rec` to `Ctx` and delete the facade.
Per file: `gfx: &mut Gfx` → `rec: &mut Recorder`, `gfx.x(` → `rec.x(`, callers pass `&mut gfx.rec` /
`&mut ctx.gfx.rec`; `TextureId` → `TexId`.

## 4. Left as TODO(step 8)

- `Ctx.gfx`: the recorder still lives inside `Gfx` (`gfx.rs` field `rec`).
- `gfx::TextureId` alias; `Mat4` / `Camera` location.
- AnmManager's `last_blend` / `bound_tex` / `last_tex_matrix` caches stay in AnmManager (they change emitted
  commands, so they are draw-phase state, not renderer caches).
- Text raster: hud / gui / menu / menulist / musicroom / ending rasterise on the CPU and `write_texture`; hud does it
  in calc. Decision 7 (text as an output command) is a later run.
- Device-loss rebuild from `ResTable` provenance (`TexSource`) is recorded but not implemented.
